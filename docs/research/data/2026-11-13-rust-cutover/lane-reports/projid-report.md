# Project identity Rust port

## Result

`project_identity.py` is fully implemented in Rust, including raw recovery identity/review, discovery, registry/history validation, observation, historical-device continuity, status, planning, preview, apply/replay, normalize and stale-lock recovery. No baseline domain function or domain branch is intentionally omitted. Argparse is replaced by typed operations, as required. The three not-yet-wired lazy dependency boundaries refuse through registered traits rather than falling back.

Acceptance on macOS arm64: `cargo fmt --check` clean; `cargo check --offline --all-targets` clean with no warnings; `t1_project_identity` passes **42/42 three consecutive times** (156.42s, 160.81s, 155.90s). No Python, Git write, network, AWA or Vox command was run.

## Authored files

- `/opt/agent-memory-worktrees/rust-v015-lane-projid/src/project_identity.rs` — new port.
- `/opt/agent-memory-worktrees/rust-v015-lane-projid/tests/t1_project_identity.rs` — new contract target.
- `/opt/agent-memory-worktrees/rust-v015-lane-projid/src/lib.rs` — alphabetical `pub mod project_identity;` plus registrations carried over with Main's dependency refresh.
- `/opt/agent-memory-worktrees/rust-v015-lane-projid/src/error.rs` — Main-approved append-only `Kind::CanonicalWriterBusy` and `Kind::CanonicalWriterInitializing`, with baseline-message constructors.
- This report.

No Cargo.toml/Cargo.lock change and no new T0 primitive. The private `canonical_writer_error` conversion in `project_identity.rs` preserves `local_registry::CanonicalWriterError` subtypes in shared `ContractError::kind()`. Main requested that this conversion eventually be owned by `local_registry`; it is deliberately private here to avoid editing that lane's file.

### Dependency refresh (not authored changes)

Per Main's explicit instruction, copied differing files from `/opt/agent-memory-worktrees/rust-v015-int` before final acceptance runs. These must not be integrated as this lane's changes:

- `src/artifact.rs`, `src/bank_contract.rs`, `src/bank_layout.rs`, `src/bank_writes.rs`, `src/fs/descriptor.rs`, `src/handoff.rs`, `src/migration_codecs.rs`, `src/migration_contract.rs`, `src/omp_installation.rs`, `src/pack_validation.rs`, `src/process.rs`, `src/records.rs`, `src/scopes.rs`, `src/sharing_limits.rs`.
- `tests/t1_a2.rs`, `tests/t1_bank.rs`, `tests/t1_migration.rs`, `tests/t2_bootstrap.rs`, `tests/t2_omp_install.rs`.
- `src/lib.rs` also carries the refreshed `bank_writes`, `migration_codecs`, `migration_contract`, `omp_installation` registrations. The original module-registration ownership rule is otherwise unchanged.

The bank-contract/layout dependency files were never independently edited.

## Public API

The interface requested by bank_transactions is exactly:

```rust
pub fn status(repo: &Path) -> crate::error::Result<crate::codec::json::Value>;
```

The explicit baseline keyword-argument interface is:

```rust
pub fn status_with_options(repo: &Path, authority: Option<&str>, options: DiscoveryOptions<'_>) -> Result<Value>;
pub fn discover(repo: &Path, authority: Option<&str>, options: DiscoveryOptions<'_>) -> Result<Value>;
pub fn evidence(repo: &Path, data: &Value, authority: Option<&str>, options: DiscoveryOptions<'_>) -> Result<Value>;
pub fn preview(repo: &Path, action: &str, target: Option<&str>, reason: &str, authority: Option<&str>) -> Result<Value>;
pub fn apply(prepared: &Value) -> Result<Value>;
pub fn recover(prepared: Option<&Value>) -> Result<Value>;
pub fn normalize(remote: &Value, base: Option<&Path>) -> Result<Value>;
```

Also public: `SPEC`; named bounds; `canonical`, `validate`, `history`, `observe`, `result`, `plan`; deterministic-source variants `plan_with_sources`, `preview_with_sources`, `apply_with_sources`, `recover_with_sources`; and `command_result(operation, Result<Value>) -> lifecycle::CommandResult` for exact receipt/exit/stdout/stderr formatting. `normalize` returns the project-result receipt; `remote_identity::normalize` remains the one string-normalization algorithm.

`DiscoveryOptions` contains owned lock/transaction bytes, an optional `MigrationReader` (`Fn(&[u8]) -> Result<Option<Value>>`), and optional `&RawIdentityEvidence`. `RawIdentityEvidence` is non-deserializable, has private fields, and can only be constructed by `observe(repo, raw, review)`; `checked(root, bank, raw)` rebinds it to independently read bytes. Immutable getters expose its root, bank, digest, metadata and retained review. Recovery review validates immutable 40/64-hex object locators, committed bytes, registered checkout and matching project ID.

### Traits and wiring

- `BankObservation::markers(bank, owned_lock, owned_transaction) -> Result<()>`: implement in `bank_observation`; registration `register_bank_observation`.
- `BankWrites::operations() -> &[&str]`: implement/register from `bank_writes`; registration `register_bank_writes`. The refresh now contains bank_writes, but this assignment's explicit registration boundary was retained. The typed-writer test uses the actual refreshed `bank_writes::OPERATIONS`, not a duplicate operation list.
- `LayoutObservation::read_source(bank, relative, limit) -> Result<Vec<u8>>`: implement in `layout_recovery_observation`; registration `register_layout_observation`.

Missing registrations are typed `ContractError` refusals. They are not silent passes. Canonical-writer Busy and Initializing refusals retain distinct shared `Kind` values through discover and status, which permits the future private-enrollment boundary to distinguish bounded retry from ordinary refusal. Bank-capacity detail fields are retained in CLI failures and replay binding-status failures.

The already-ported modules are called directly (`local_registry`, `repository_config`, `remote_identity`, `identity_git`, `developer_identity`, `host`, `bank_contract`). CLI wiring remains responsible for registering repository_config's security scanner as in that existing module.

## Durable compatibility

- Uses the real `projects.json` and `.projects.lock` paths from `local_registry::paths("projects")`.
- Registry/proposal hashing uses the existing `developer_identity::encoded`: sorted keys, UTF-8, two-space indent, finite numbers, trailing LF. Load rejects duplicate keys as baseline `unique_object` does.
- Observation/status never re-encode or rewrite existing registry bytes.
- Mutation uses `local_registry::exclusive` and `replace_with_sources`: exclusive create plus the existing EX **flock** class, same owner fields (`operation_id`, `operation`, `host`, `pid`, `start`), same project operation names, mode 0600/private directory 0700, observed-byte comparison, durable replacement and readback.
- Recovery uses the existing two-step, dead-local-owner protocol and exact `agent-memory/project-lock-recovery-v1` schema. Live owners refuse; preparing recovery leaves bytes and lock in place; applying verifies owner bytes/inode and preserves registry bytes.
- Historical alias adoption stays read-only. Explicit link preserves old alias/history and publishes the observed alias; rebind/fork require the link first. Numeric device ordering does not narrow Python's arbitrary-sized registry numbers to machine integers.
- Proposal evidence comparison is object-order-independent, including proposals serialized by Python's sorted-key encoder.

## CLI declarations for the CLI lane

Required subcommand, with no mutually exclusive groups:

- `normalize REMOTE [--base BASE]`.
- `status [--repo REPO] [--authority AUTHORITY]`; repo default `.`.
- `preview [--repo REPO] [--authority AUTHORITY] --action ACTION [--project-id ID] --reason REASON`; repo default `.`; action choices `create`, `bind`, `link`, `rebind`, `fork`.
- `apply --proposal FILE`.
- `recover [--proposal FILE]`.

Success is exit 0, UTF-8 indent-2 JSON plus LF, empty stderr. Typed operation refusal is exit 1, baseline compact/default JSON result plus LF, empty stderr; capacity details are merged when present. Input-file acquisition is CLI-owned. `normalize` receipt is available without discovery.

## Obligation coverage

Each ported obligation has its exact ledger ID immediately above its Rust test. Ledger itself was not modified (not owned). All paths below use the `tests/contracts/` prefix and the class indicated.

### ProjectIdentityContract

23 domain obligations are fully ported to `tests/t1_project_identity.rs`:

| Baseline method in test_project_identity.py | Rust test |
|---|---|
| test_bootstrap_is_read_only_until_explicit_local_registration_and_bank_initialization | bootstrap_readonly_then_explicit_registration |
| test_device_change_recovers_readonly_and_explicit_link_persists_alias | device_change_recovers_and_link_persists_alias |
| test_historical_fork_and_rebind_require_explicit_alias_link | historical_fork_and_rebind_require_link |
| test_observed_alias_registered_after_preview_refuses_and_retains_history | concurrent_alias_registration_refuses_stale_preview |
| test_old_pending_proposal_evidence_requires_fresh_preview | old_pending_proposal_requires_fresh_evidence |
| test_historical_candidates_are_deterministic_and_conflicts_refused | historical_candidates_sort_numerically_and_conflicts_refuse |
| test_rebound_aliases_after_later_device_change_require_explicit_selection | rebound_historical_aliases_require_explicit_selection |
| test_historical_recovery_requires_bank_authority_location_and_inode | historical_recovery_requires_all_continuity_evidence |
| test_historical_recovery_preserves_explicit_authority | historical_recovery_preserves_explicit_authority |
| test_normalization_strips_secrets_and_unifies_only_documented_equivalence | normalization_strips_secrets_and_preserves_distinctions |
| test_linked_worktrees_join_via_the_real_common_git_directory | linked_worktrees_share_common_directory_identity |
| test_no_remote_move_keeps_id_and_preserves_location_history | moved_repository_preserves_location_history_and_old_replay |
| test_copied_bank_requires_explicit_link_or_fork | copied_bank_requires_link_or_fork |
| test_fork_on_fresh_machine_is_caught_by_shared_authority_evidence | fresh_machine_fork_requires_shared_authority_decision |
| test_same_remote_on_fresh_machine_binds_stable_bank_id | fresh_machine_matching_remote_binds_stable_id_without_secrets |
| test_multiple_remotes_require_authority_and_configured_selection_wins | configured_authority_resolves_multiple_remotes |
| test_explicit_authority_is_retained_and_conflicting_push_url_refuses | explicit_authority_persists_and_conflicting_push_refuses |
| test_remote_change_stops_until_explicit_rebind_and_retains_alias_history | rebind_retains_alias_history_and_never_reapplies_old_decision |
| test_changed_registry_bank_remote_or_authority_refuses_stale_proposals | every_changed_source_refuses_stale_proposals |
| test_parallel_registrations_preserve_winner_and_replay_is_idempotent | parallel_registrations_preserve_winner_and_replay |
| test_canonical_remote_does_not_merge_different_bank_ids | canonical_remote_never_merges_different_bank_ids |
| test_unknown_bank_symlinks_and_busy_state_refuse_without_mutation | unknown_bank_symlinks_and_busy_state_preserve_sources |
| test_project_registry_stale_lock_recovery_is_bounded_and_read_back | stale_lock_recovery_proves_dead_owner_and_readback |

The parallel-registration case races independent registry writers in Rust threads with independent flock descriptors inside an isolated test process. It asserts exactly one winner and byte-preserving replay.

Partially ported:

- `test_copied_pack_spaces_non_git_project_and_unrelated_registry_preserved` -> `non_git_spaces_and_unrelated_registry_preserved`: spaces/non-Git/foreign-registry preservation complete; copying and executing a Python helper is intentionally not reproduced. Native relocated-executable/package qualification belongs to the CLI/release lane.

Deferred because they require `scope_binding`, not because project identity is missing:

- `test_scope_link_repair_preserves_bank_and_verifies_current_alias`.
- `test_conflicting_historical_aliases_allow_explicit_scope_link`.

### RepositoryObservationBatch

The requested project-identity subset is ported for these 13 methods of `test_repository_observation_batch.py`:

| Baseline method | Rust test |
|---|---|
| test_real_root_spellings_and_linked_worktrees_preserve_public_identity | real_root_spellings_and_linked_worktrees_preserve_identity |
| test_sha256_head_retains_committed_locator_after_staged_deletion | sha256_committed_locator_survives_staged_deletion |
| test_unborn_root_preserves_local_settings_and_indexed_bank_identity | unborn_root_preserves_indexed_bank_identity |
| test_non_git_directory_remains_readable_without_inventing_identity | non_git_directory_never_invents_identity |
| test_grouped_discovery_keeps_effective_includes_rewrites_and_remote_conflicts | grouped_discovery_preserves_includes_rewrites_and_conflicts |
| test_corrupt_missing_and_dangling_head_never_establish_empty_repository | damaged_head_never_establishes_an_empty_repository |
| test_unreadable_or_malformed_metadata_preserves_refusal_and_original_bank | malformed_git_metadata_preserves_bank_and_refusal |
| test_malformed_successful_observation_never_resolves_existing_identity | malformed_successful_root_observation_never_resolves |
| test_signaled_or_incomplete_failed_observation_never_adopts_partial_root | failed_root_observation_never_accepts_partial_output |
| test_unavailable_commit_or_locator_blob_never_becomes_absent_memory | unavailable_commit_or_locator_blob_never_means_absent_memory |
| test_coalesced_observation_ignores_ambient_authority_and_fsmonitor | ambient_git_authority_and_fsmonitor_are_ignored |
| test_invalid_config_refuses_before_speculative_real_git_inventory_acceptance | invalid_local_configuration_preserves_bank_and_sanitizes_refusal |
| test_branch_validation_remains_after_competing_router_refusal | competing_router_refusal_precedes_branch_validation |

For whole-ledger accounting, treat these as partial wherever the baseline additionally asserts scope_binding status/enrollment, direct git_roots results or Python monkey-patched child-handle cleanup. Those assertions are not faked here. Grouped discovery and competing-router refusal have their complete domain outcomes represented. The malformed/signal observations use real isolated shell Git shims, not Python, while fixture setup still uses real Git.

Other methods in this broad repository_config/scope suite are not claimed as migrated by this lane:

- Waiting on scope_binding/enrollment (and the named hold modules where applicable): `test_changed_head_with_deleted_competing_router_refuses_enrollment_exit`, `test_config_and_remote_mutation_between_enrollment_entry_exit_are_fresh`, `test_index_untracked_and_raw_locators_added_after_scope_read_refuse_exit`, `test_real_verified_hold_remains_readonly_through_grouped_settings_and_enrollment`, `test_subdirectory_still_refuses_repository_root_authority`, `test_tracked_body_and_enrollment_boundaries_have_no_live_query_children`.
- Repository_config/identity_git-owner tests outside this lane's project subset: `test_earlier_real_config_refusal_beats_every_later_child_failure`, `test_independent_git_roots_resamples_head_and_validates_its_own_observation`, `test_interrupt_in_real_config_validation_preserves_traceback_and_cancels_siblings`, `test_malformed_config_child_output_never_accepts_speculative_names`, `test_malformed_name_children_refuse_after_valid_real_config`, `test_parent_traversal_name_never_adopts_outside_router`, `test_real_config_validation_time_is_charged_to_unresolved_inventory_child`, `test_sampled_head_is_immutable_and_next_settings_read_observes_later_commit`.
- `test_remote_query_cleanup_failure_cannot_become_successful_discovery`: implementation-only in this lane for the parent-close-failure injection branch; discovery only degrades `identity_git` contract refusals into inconsistent remotes and propagates all infrastructure/cleanup failures. Rust T0 owns the actual child/pipe handles, so Python's direct Popen/ClosingPipe monkey-patch is not reproduced. This branch is not claimed as an independently exercised fault test here.
- `test_timeout_and_launch_failure_never_mint_identity_from_partial_output`: timeout/launch-injection and private-enrollment assertions remain with identity_git/scope_binding. The project subset exercises malformed, failed and signaled real root observations but does not claim these additional injected variants.

### Additional tests (no baseline obligations claimed)

- `registry_bytes_validation_and_unordered_proposal_maps`: baseline literal empty-registry encoder bytes, persisted sorted-key proposal comparison, reused operation-ID refusal, schema-field refusal.
- `raw_identity_requires_independent_typed_evidence_and_providers`: missing trait registration refuses, actual pinned source reads, digest/path rebinding and raw/migration exclusivity.
- `committed_identity_review_establishes_existing_checkout_only`: real committed source, retained damaged bytes, immutable locator and checkout/project matching.
- `owned_markers_and_migration_reader_preserve_authority`: owned marker bytes, changed transaction, supported/unsupported migration readers.
- `canonical_writer_busy_and_initializing_remain_distinct_typed_refusals`: real EX flock owner and empty initializing slot; both discover/status retain the approved shared Kind variants.

Test registration adapters exercise their stated boundaries only: repository label scan, pinned filesystem recovery reads and owned-marker checks. They do not claim implementation/coverage of later modules.

## Declared deltas / bounds

- Ordinary router reads are explicitly bounded at 1 MiB; ordinary router contract validation retains bank_contract's 16,384-byte bound. Raw recovery already had the 1 MiB baseline bound. Marker ownership reads are bounded at 1 MiB; registry reads at the shared 64 MiB limit. Oversized reads refuse without rewriting or truncating the source.
- Non-UTF-8 paths/Git textual output/router bytes are refused rather than reproducing Python surrogateescape or interpreter exception details. Strict JSON inherits the shared BOM/UTF-16/UTF-32/lone-surrogate/depth refusals from the design.
- Malformed request types produce typed Rust refusals instead of reproducing Python KeyError/TypeError/AttributeError formatting. UUID and action domain decisions retain their explicit baseline messages.
- No Python argparse/help layout or executable .py compatibility aliases; typed CLI handover above.
- Python-era adoption format/paths/locks are implemented and covered by on-disk bytes/ownership tests. A genuine v0.0.14 host adoption qualification remains release-owned, not claimed by this source-level lane.

## Verification commands and final results

1. `cargo check --offline --all-targets` before dependency refresh: passed; only the known unrelated t1_a2 unused-import warnings.
2. Initial `cargo test --offline --test t1_project_identity`: 30/33 passed; three **outer fixture process launches** failed macOS Spawn/EOVERFLOW. Reported to Main; Main accepted a T0 pre-fork FD snapshot robustness defect. No production retry or T0 edit was added here.
3. Refreshed the non-owned src/tests files as Main requested and re-read the lane brief's Added rules.
4. `cargo check --offline --all-targets`: passed without warnings.
5. `rustfmt --edition 2024 --config skip_children=true src/project_identity.rs src/error.rs src/lib.rs tests/t1_project_identity.rs && cargo fmt --check`: passed; only authored files were formatted.
6. `cargo test --offline --test t1_project_identity`: **42 passed**, run 1, 156.42s.
7. `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/project_identity.rs src/error.rs`: only `project_identity.rs:36 .expect("bundled project schema")`; its invariant is explicitly documented immediately above the decode: a committed bundled asset, never runtime input. No input-derived panic sites.
8. `cargo check --offline --all-targets && cargo fmt --check && for run in 2 3; do printf '\nIdentity acceptance run %s\n' "$run"; cargo test --offline --test t1_project_identity --quiet || exit "$?"; done`: checks clean, **42 passed** run 2 (160.81s), **42 passed** run 3 (155.90s).

All test cases use canonicalized 0700 temporary roots and isolated HOME/config environments with real Git and filesystem behavior. They never mutate the parent process environment. `OUTER_LAUNCH` serializes every outer fixture subprocess launch because of the reported macOS T0 snapshot defect; this is why waiting outer Rust tests can print over-60-second notices. The actual parallel-registration race remains concurrent inside its test process. Main may remove this outer serialization after integrating its T0 fix.
