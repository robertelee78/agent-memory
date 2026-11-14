# Admission lane — checkpoint 1

Coordinator-directed split: this checkpoint contains `trust_paths`, `admission`, `am capture`, and `am admission`. Main explicitly deferred `memory_refresh`, `am refresh`, and `am boundary` until Retrieval is integrated. None of those deferred files/exports/routes has been added. This is not a claim that the original three-module lane is complete.

## Owned changes

- New: `src/trust_paths.rs`, `src/admission.rs`, `src/cli/helpers/admission.rs`, `tests/t2_admission.rs`, `tests/support/admission.rs`.
- Shared appends: `src/lib.rs` (`admission`, `trust_paths`); `src/error.rs` (`Kind::PrivateInput` and its constructor); `src/bank_writes.rs` (`RecoveryFrontier::require_scope_owned`); `src/cli/helpers/mod.rs`; `src/cli/table.rs`; `src/cli/mod.rs` (capture validator registration); `tests/wiring-pending.txt` (four admission dependencies; transaction-validator pending entry removed because capture is now registered).
- `tests/obligations/python-baseline-v1.json`: 5 newly ported and 7 newly partial obligations, only from this target.
- No Git/GitHub writes, Python execution, new crates, unsafe code, or changes to baseline Python.

The trust_paths module deliberately re-exports the complete existing `fs::trust` implementation rather than duplicating policy. Admission ports request validation, classification, safe auditing, drafting, incomplete discovery start, fresh capture validation, desired canonical files/history, exact evidence and event times, preview, source and transaction validation, apply/historical receipt handling, and protected input reads. CLI input errors use the baseline private-workspace guidance; apply errors retain the source-transport-busy exception and recovery uncertainty. The helper implements the baseline recovery-bound private-preview output policy.

## Public APIs and seams

`trust_paths` exports `InspectOptions`, `Inspection`, `TreeInspection`, `inspect_entry`, `inspect`, `refuse`, `tree`, `source`, `git_pointer`, `state`, `record`.

Admission public functions: `result`, `request_value`, `classify`, `capture`, `draft`, `draft_with_sources`, `start`, `capture_value`, `desired`, `evidence_value`, `preview`, `validate_transaction`, `check_sources`, `apply`, `read_input(path, maximum)`, `read_input_observed`, `command_result`. `Admission` implements `bank_transactions::TransactionValidator` and is registered for `capture`. `read_input_observed` exposes a synchronous observation callback for deterministic real-filesystem race tests; ordinary `read_input` uses the same path with an empty callback.

New required registration traits (all unavailable providers return typed MissingProvider):
- `admission::Comparison`: request validation and owned comparison check; implementer `comparison`.
- `admission::Discovery`: start; implementer `discovery`.
- `admission::AdmissionNavigation`: navigation validation/application; implementer `admission_navigation`.
- `admission::LayoutResolution`: evidence_fields/claims_for; implementer `layout_resolution`.

Existing trait extended, approved by Main and shared with LayoutCore:

```rust
fn require_scope_owned(
    &self,
    repo: &Path,
    project: &Value,
    expected: Option<&Value>,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<Value>;
```

This is appended to `bank_writes::RecoveryFrontier`, without a default. No implementer existed in this snapshot, so no non-owned fixture needed modification. The forthcoming recovery_frontier provider must delegate to its baseline-equivalent owned require_scope.

`PrivateStateHooks` cannot split cleanly: it is one eight-method trait spanning installation, OMP, and admission. Per Main's decision it remains unregistered. Its eventual composite provider must delegate `read_input` to `admission::read_input(path, maximum)`; runtime_root is not needed by this reader.

No second Security seam: admission calls the integrated `security::{scan, repository_source, audit}` directly. The integrated `audit` already supports the required source classes, reasons, and record IDs.

The operation-keyed transaction registry now wires **capture only**. Consolidation/governance/initialize/refresh remain required future implementations; their absence still returns MissingProvider, not approval.

## Contracts

`tests/t2_admission.rs`: 16 tests: 10 passing, 6 ignored because the production recovery/comparison/discovery/layout-resolution providers are unavailable. No provider mocks were installed.

Fully ported and executed (all five AdmissionPrivacyContract obligations):
- test_private_input_and_refuse_readable_or_redirected
- test_replacement_during_read_is_refused
- test_platform_temporary_alias_with_private_directory
- test_intermediate_symlink_is_refused
- test_public_cli_explains_private_workspace

Partial obligations (ledger marked partial):
- AdmissionContract.test_unmerged_capture_requires_actual_conditions_and_never_claims_code_landed: executed unknown-applicability rejection/control; full Git-context admission journey not yet ported.
- AdmissionContract.test_user_correction_admits_small_record_and_history_automatically_without_git_changes: executed exact desired-state record/body/origin/history and collision tests; complete apply/replay/external-edit journey is implemented but ignored pending providers.
- AdmissionContract.test_draft_fills_identity_without_deciding_or_writing: real production identity/bank journey implemented, ignored pending recovery_frontier.
- AdmissionContract.test_start_keeps_semantic_review_pending_and_never_writes_bank: implemented complete eligible/pending/quarantine journey, ignored pending recovery_frontier/comparison/discovery.
- AdmissionContract.test_unconfirmed_proposals_and_weak_sources_never_become_confirmed_decisions: implemented weak-source/procedure/inference and forged-capture refusal journey, ignored pending recovery_frontier.
- AdmissionContract.test_unsafe_and_raw_conversation_capture_is_quarantined_without_content_copies: implemented capture, content-free response/audit, canonical/Git preservation, ignored pending recovery_frontier.
- AdmissionContract.test_transient_redundant_source_and_tracker_copies_are_discarded: implemented all three flags and preservation, ignored pending recovery_frontier.

Additional executed controls: bounded/single-link/strict-UTF8 protected input, exact explicit assessment and small first-version candidate, malformed request/capture/evidence no-panic refusals.

The following baseline AdmissionContract tests have not yet been translated in checkpoint 1 (not marked in the ledger):
- test_large_existing_inventory_admits_or_finishes_recovery_without_losing_prior_learning
- test_repository_and_reviewed_tool_evidence_are_bound_to_actual_bytes_and_preserved
- test_duplicate_learning_defers_to_existing_identity_and_peer_conflict_to_detecting_developer
- test_arrived_learning_tampered_delta_and_changed_evidence_refuse_before_writing
- test_interrupted_admission_rolls_back_before_fresh_comparison_even_if_source_changed
- test_evidence_change_at_final_readback_cannot_get_a_verified_admission_receipt
- test_terminal_deleted_identity_and_derivations_are_quarantined_not_recaptured
- test_recorded_admission_recovers_lost_response_without_replaying_or_requiring_new_evidence
- test_completed_receipt_cleanup_does_not_trust_corrupt_receipts_or_overwrite_external_memory
- test_complete_receipt_schema_and_bindings_govern_cleanup_and_historical_replay

All memory_refresh baseline functions and their contract tests remain outside this coordinator-directed checkpoint, awaiting Retrieval integration; no narrower implementation is presented as parity.

## Declared deltas / foundation changes

- UTF8/BOM/surrogate/nesting/reader bounds follow the binding Rust codec policy rather than accidental Python decoder behavior.
- The protected reader additionally rechecks held ancestor access-policy signatures, rejecting chmod/ownership changes during observation rather than blessing a stale private barrier.
- Input race qualification uses real renames and descriptor readback, not Python monkeypatching.
- No foundation implementation change: only reuse of fs::trust and pinned descriptor readers. The sole existing domain interface extension is the approved owned recovery scope method above.

## Approved integration refresh

Started from 46a0100. Main authorized copying every changed non-owned file from a725b97, preserving this lane's appends. The initial text-tool refresh removed literal U+0085 NEL bytes in a security fixture, producing an observed security-vector failure. At Main's explicit direction **every refreshed file below was restored using byte-preserving `git show ... > path` shell redirection and compared successfully to the committed bytes with `cmp`**, after which only the lane's shared appends and ledger markings were reapplied. The fixture's observed final SHA256 and committed SHA256 both equal `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41`. This refresh transport defect was reported to xd://report_issue. Refreshed exact paths:

Cargo.lock; Cargo.toml;
docs/research/data/2026-11-13-rust-cutover/lane-reports/ompstream-report.md;
docs/research/data/2026-11-13-rust-cutover/lane-reports/recallcatalog-report.md;
docs/research/data/2026-11-13-rust-cutover/lane-reports/security-report.md;
src/cli/helpers/mod.rs; src/cli/helpers/security.rs; src/cli/mod.rs; src/cli/table.rs;
src/error.rs; src/host.rs; src/lib.rs; src/model_reference.rs;
src/omp_stream.rs; src/omp_stream/clock_window.rs; src/omp_stream/coverage.rs; src/omp_stream/executor.rs; src/omp_stream/frame.rs; src/omp_stream/inventory.rs; src/omp_stream/linux_image.rs; src/omp_stream/run.rs; src/omp_stream/wake.rs;
src/process.rs; src/recall_catalog.rs; src/security.rs;
tests/obligations/python-baseline-v1.json;
tests/t1_a1.rs; tests/t1_bank_transactions.rs; tests/t1_layout_history_workspace.rs; tests/t1_layout_recovery_state.rs; tests/t1_private_state.rs; tests/t1_project_identity.rs; tests/t1_scope_binding.rs; tests/t1_security.rs; tests/t2_layout_recovery_observation.rs; tests/t2_omp_stream.rs; tests/t2_recall_catalog.rs; tests/t_support_world.rs; tests/vectors/security-regex-v1.json; tests/wiring-pending.txt.

## Commands and evidence

Platform: aarch64-apple-darwin, Darwin 27.3.0 arm64, rustc 1.99.1 (58cf8f9c3, 2026-10-13), LLVM 23.1.1. Linux targets were not run.

- `awa status --json`: managed robertelee78/agent-memory. Tracker writes remain Main's responsibility under the explicit lane contract.
- `vox agent status --harness omp`: omp-work Ready, repository room kh57aq2zwdss.
- `rustfmt --edition 2024 --emit stdout ...` and `--config skip_children=true`: formatting generated to temporary output and applied through edit; no formatter rewrote non-owned files.
- Initial `cargo fmt --check` identified only lane additions; corrected.
- Initial `cargo check --offline --all-targets --features dev-tools` refused missing security before the approved refresh. After a725b97 refresh: succeeds, zero warnings.
- First admission test run found two fixture defects (template project UUID not replaced; platform test assumed TMPDIR was /tmp). Both fixed without narrowing assertions.
- `cargo test --offline --features dev-tools --test t2_admission` three consecutive final runs: each 10 passed, 6 ignored, zero failures.
- `~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh /tmp/admission-ported-names.txt tests/t2_admission.rs`: marked ported=5 partial=7 in this worktree.
- Final `cargo fmt --check`: clean.
- Final `cargo check --offline --all-targets --features dev-tools`: succeeds, zero warnings.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/trust_paths.rs src/admission.rs src/cli/helpers/admission.rs`: no matches (grep exit 1).
- First whole-suite invocation was terminated by the outer tool's 300-second deadline before the suite finished; no Rust test failure was reported before that interruption. The exact suite command was then launched under a 1,800-second outer allowance (no domain child deadline changed).
- The second whole-suite run completed in 513 seconds with one failed target: the NEL-corrupted `t1_security::captured_cpython_regex_vectors_preserve_full_sequential_scan`; all other targets passed or retained their declared ignores. The refresh transport was corrected as described above, and fmt/check plus all three admission repetitions passed again.
- Final corrected `cargo test --offline --no-fail-fast --features dev-tools`: **green**, 908 passed across 60 suites, 55 declared ignores; outer wall time 512.42 seconds. Retained output: artifact://12674. Final admission repetitions each passed 10 tests with 6 declared ignores; fmt clean and all-targets check zero warnings.
