# Layout recovery / retention lane

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-layoutrecovery`.
Starting baseline: detached `8f69d726`; approved prerequisite refresh: `fba4152d`.
No Git/GitHub writes, Python execution, subagents, or new crates.

## Implementation

Read both assigned Python modules as specifications, plus their recovery and retention contracts. Reviewed the previous lane's retention implementation and publication registration helper against that specification and retained them. Completed the missing layout recovery implementation.

All assigned baseline functions are implemented: exact schemas/participants, independent review, manifest and original-blob verification, inspection and bounded base64 pages, complete semantic unit coverage/context mapping, provenance/record preservation, metadata plans, pending-source extraction, preparation and immutable proposals, predecessor handoff, transaction/hold/sentinel markers, identity/Git/runtime rebinding, internal RecoveryReadSet-authorized replacement, ordered action and rollback boundaries, probes, progress/journal/receipt durability, interrupted finish/rollback, historical non-replay, current status, child-restore dispatch, qualified recall, safe public envelopes, private-output preflight/export, and CLI failure envelopes. Retention covers complete owned-artifact inventory, references, real parent/child evidence, probe/nested/resolution/publication/operator dependencies, separate source-bound review, exact-file unlink, attainable unlink-before-progress resume, tombstones and historical receipts.

The existing `bank_writes::RecoveryFrontier` interface is used throughout. Recovery frontier itself was not ported. Missing providers fail closed; no fake frontier or publication implementation was added.

## Lane-owned changes

- `src/layout_recovery.rs`
- `src/layout_recovery/command.rs`
- `src/layout_recovery/journal.rs`
- `src/layout_recovery/mutation.rs`
- `src/layout_recovery/publication.rs` (inherited implementation retained/formatted)
- `src/layout_recovery/validation.rs`
- `src/layout_recovery_retention.rs` (inherited implementation retained/formatted)
- `src/layout_recovery_context.rs` (approved append-only private read-only accessors)
- `src/bank_writes.rs` (required registration-method appends)
- `src/host.rs` (supplementary process groups)
- `src/lib.rs` (module declarations)
- `src/cli/helpers/layout_recovery.rs`
- `src/cli/helpers/bank_contract.rs`
- `src/cli/helpers/mod.rs`
- `src/cli/mod.rs`
- `src/cli/table.rs`
- `tests/t2_layout_recovery.rs`
- `tests/wiring-pending.txt`
- `tests/obligations/python-baseline-v1.json`

Main also authorized byte-exact prerequisite copies from `fba4152d`. These are integration snapshots, not independent lane implementations: checkpoint/inactive and retrieval reports; admission, bank_transactions, checkpoint, discovery, error, handoff, inactive, installation/providers, projections, retrieval, security and their CLI helpers; tests/support/mod.rs, t1_a2, t1_checkpoint_inactive and t2_retrieval. Shared bank_writes/host/lib snapshots were copied before reapplying the lane's appends. Refreshed wiring/helper/table/ledger files were then extended as listed above. The security vector hash was observed as `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41`.

## APIs and registration

Typed public functions retain the baseline names. Every operation requiring runtime authority receives an explicit runtime root; observations/receipts use the existing `layout_restore::Operation` and `RecoverySource` structures. The CLI accepts a typed `layout_recovery::Command` and returns `lifecycle::CommandResult`.

Production composition now calls:

- `bank_contract::register_route_table` with `cli::helpers::layout_recovery::RouteTable`.
- `layout_restore::register_layout_recovery` with `layout_recovery::LayoutRecovery`.

Both corresponding pending lines were removed. The exact nested `bank` route is `recover-layout`, with no prefix arguments. The direct helper-stem route `am recover-layout` is also declared. Subcommands are inspect, prepare, apply, status, recover, rollback, resolve, revise-units, recover-nested, retention inspect/apply, and publish inspect/prepare/land. No executable Python aliases are installed by this lane.

Required RecoveryFrontier methods appended: rolled_back_predecessor, inherited_pending_rows, prospective_token, install. Required RecoveryTransactions methods appended: nested_receipt, recover_nested. Their future implementers remain recovery_frontier / recovery_transactions. The inherited LayoutPublication registration requires operation_evidence, inspect, prepare, land; `tests/wiring-pending.txt` records its future layout_recovery_publication provider.

Foundation addition: `host::groups()` wraps bounded native getgroups calls. Unsafe remains inside host.rs. ReadSet fields remain private; root/manifest/proposal/transaction accessors are crate-private, and construction still requires the existing internal checked observer.

## Obligations

The ledger marker reported **6 ported, 15 partial** for this target. Global ledger summary after marking: partial 420, ported 295, unported 1147.

Fully ported, executable obligations from `tests/contracts/test_layout_recovery_retention.py::LayoutRetentionContract`:

- test_unknown_backup_even_empty_directory_is_not_disposable
- test_resolution_and_quiescence_metadata_remain_retained
- test_owned_incomplete_staging_is_retained_not_promoted_to_complete
- test_symlink_hardlink_and_unsafe_mode_never_gain_cleanup_ownership
- test_individual_unlink_keeps_receipt_and_directories_and_checks_changed_bytes
- test_resume_accepts_only_the_one_unlink_before_progress_boundary

Additional executing tests cover independent-session attribution, complete unit bytes/context/target binding, safe public redaction, supplementary groups, and real-file attainable forward/rollback prefixes.

Partial obligations have executable journey bodies but are deliberately ignored with the required `needs recovery_frontier registered` annotation; **none was executed or claimed green**. What remains is real-provider integration and running/asserting the entire public journey, not a stand-in proof:

From `test_layout_recovery.py::LayoutRecoveryContract`:

- test_all_preserve_apply_keeps_metadata_feature_head_and_exact_original_index
- test_exact_retry_is_historical_and_never_overwrites_later_context
- test_prepare_requires_full_coverage_independent_review_real_identity_and_safe_output
- test_changed_source_or_metadata_invalidates_apply_before_any_canonical_replacement
- test_completed_restore_has_distinct_child_and_keeps_parent_historical_receipt
- test_initial_quiescence_requires_exact_independent_review
- test_partial_original_staging_remains_incomplete_and_fresh_inspection_preserves_it
- test_first_replacement_before_progress_supports_known_finish_and_rollback (both modes)
- test_known_finish_and_rollback_follow_exact_journal_boundaries (finish/rollback matrix, excluding reversal of a durable finished receipt as in baseline)
- test_reported_sizes_fail_normal_gate_and_public_pages_reconstruct_every_original
- test_prepare_refuses_demoting_canonical_context_or_typed_prior_guidance

From `test_layout_recovery_retention.py::LayoutRetentionContract`:

- test_public_pending_units_cannot_be_deleted_by_retention
- test_public_terminal_cleanup_crash_resume_and_real_historical_retry
- test_public_child_restore_cleanup_uses_real_parent_receipts_without_child_ledger
- test_public_purge_and_age_cannot_retire_pending_hold

Retention public journeys additionally need layout_recovery_publication registered for terminal operation evidence. Existing raw-file/depth/count/aggregate/alias obligations remain covered by the already-ported `tests/t2_layout_recovery_observation.rs`; they were not falsely re-marked as new work here. Broader generated crash, nested, admission, comparison and publication contract suites are not claimed as covered by this lane target.

## Declared deltas and qualifications

- Explicit named reader/listing limits and strict UTF-8/JSON decoder behavior follow the binding Rust design; unbounded/non-UTF-8/interpreter-specific accidental behavior is not retained.
- No argparse/interpreter execution: typed Rust parser declarations preserve the command names/options and domain-safe outputs; usage/help rendering follows the existing native CLI convention. CLI page numbers are bounded native i128 inputs rather than arbitrary Python integers.
- The executing runtime/binary is explicitly receipt-located instead of inferred from Python `__file__`; unregistered publication/frontier/nested providers produce typed refusals.
- Directory creation uses the existing foundation primitive, which establishes exact ordinary permissions on a newly created directory and never repairs an existing inode. Existing file metadata is checked rather than silently changed.
- All ignored journeys are integration qualifications, not evidence of release readiness. There are no intentionally omitted assigned implementation functions or no-op fallbacks.

## Verification

Platform observed: Darwin 27.3.0 arm64; host triple aarch64-apple-darwin; rustc 1.99.1 (58cf8f9c3, 2026-10-13), LLVM 23.1.1. Linux/musl targets were not run.

Final observed commands:

- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: success, zero warnings.
- `cargo test --offline --features dev-tools --test t2_layout_recovery`: green three consecutive runs, each **11 passed, 15 ignored**.
- `~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh /tmp/layoutrecovery-ported-names.txt tests/t2_layout_recovery.rs`: success; the temporary selector file was removed afterward.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/layout_recovery.rs src/layout_recovery_retention.rs src/layout_recovery/*.rs`: only retention.rs:97, the documented invariant `expect("constant retention pattern")`; no input-derived unwrap/expect or unchecked slices. Base64 alphabet indexes are bounded six-bit values with their local invariant documented.
- `uname -srm; rustc -vV`: platform details above.

Earlier compile/test iterations found and corrected missing optional resolution arguments, private test-helper imports, SHA display formatting, an unused accessor/import, and a test-only object-order comparison. These are not outstanding failures.

Whole-suite command `cargo test --offline --no-fail-fast --features dev-tools` completed on macOS in 599 seconds with exit 101. **The whole-suite finish line is not met:** `tests/t2_lifecycle.rs` had 36 failures, all at line 434 while unwrapping `Release metadata exceeds its bound` during common release-fixture setup. Every other test target completed successfully, including `t2_layout_recovery`, the existing layout core/observation targets, obligation-ledger consistency and production registration wiring. Full output: `artifact://13452`.

This external failure was reported directly to Main with file/line and retained output. No release metadata bound or non-owned lifecycle test was changed, and the failing target was not rerun to confirm the reported result. Main owns assignment/integration of that fix. Lane implementation and reachable verification are complete, but neither overall release readiness nor the ignored real-frontier journeys are claimed.

Accountability: `awa status --json` observed a managed repository; GitHub and Git writes remain exclusively Main's responsibility under the lane contract. Vox reported the OMP node Ready in the bound repository room. No independent tracker phase or publication claim is made by this worker.

Main's subsequent diagnosis: the signed release-v2 `command_surface` grows with registered routes; integration was already approximately 46 KiB, and these routes push the test manifest beyond the shared 64 KiB `MAX_METADATA` bound. Main confirmed this is not a lane defect and is introducing a separate manifest bound in integration. Per Main's direction, hand over this lane unchanged; Main will verify it on top of that fix.
