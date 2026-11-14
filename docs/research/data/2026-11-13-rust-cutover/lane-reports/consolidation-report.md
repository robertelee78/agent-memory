# Consolidation lane report

## Scope and implementation

Ported the complete read-only baseline specifications `comparison.py` and `consolidation.py`, including comparison review/check, capture-identity provenance and legacy flat-path ambiguity, pending recovery coverage, detached output plans, meaning-preserving consolidation, lifecycle/history/router updates, exact transaction rebuilding, source revalidation and receipt-based replay. No Python was executed and no crates or Git/GitHub writes were introduced.

New files:
- `src/comparison.rs`
- `src/consolidation.rs`
- `src/cli/helpers/comparison.rs`
- `src/cli/helpers/consolidation.rs`
- `tests/t2_consolidation.rs`

Lane-owned shared changes:
- `src/lib.rs`: alphabetical module declarations.
- `src/cli/helpers/mod.rs`: helper declarations.
- `src/cli/table.rs`: baseline-order `compare` then `compact`, before `checkpoint`.
- `src/cli/mod.rs`: comparison providers and `compact` validator registration.
- `tests/wiring-pending.txt`: removed both comparison-provider pending lines.
- `tests/obligations/python-baseline-v1.json`: 26 annotated baseline obligations marked partial.

## API and wiring

`comparison`: `safe_text`, `request_value`, `inspect`, `judge`, `review`, `check`, `read_input`, `cli_judgment`, `command_result`, and `Comparison`. Optional held-writer context is preserved on the source-bound operations. `Comparison` implements the existing `admission::Comparison` and `discovery::Comparison` traits; both are registered by the CLI. No new seam was introduced.

`consolidation`: `result`, `reference`, `child_id`, `union`, `fingerprint`, `input_value`, `inspect`/`Inspection`, `route`, `routed`, `build`, `plan`, `preview`, `validate_transaction`, `check_sources`, `apply`, `command_result`, and `Consolidation`. `Consolidation` implements `bank_transactions::TransactionValidator` and is registered for `compact`.

CLI: `am compare inspect|review|check` preserves request, judgment/proof, optional coverage review and private-output arguments; `am compact plan|preview|apply` preserves protected input/proposal, detached plan, operation ID/time and private-output arguments. All domain functions call the ported retrieval, discovery, security, admission, layout resolution and transaction modules directly. Recovery-frontier calls reuse `bank_writes::RecoveryFrontier`, including `pending_rows`; no frontier trait additions were necessary.

## Obligations and deferred execution

All 13 `ComparisonContract` methods in `tests/contracts/test_comparison.py` and all 13 `ConsolidationContract` methods in `tests/contracts/test_consolidation.py` have exact obligation annotations. The ledger honestly records all 26 as partial: pure structural/semantic/build contracts execute, but the full baseline filesystem/qualification/recovery journeys require the still-unported recovery-frontier provider. No claim of complete baseline acceptance is made.

The test target has 24 tests: 13 executing and 11 explicitly ignored with `needs recovery_frontier registered`:
- `arriving_memory_tampered_delta_and_checkout_require_new_plan`
- `higher_order_pattern_stays_proposal_only`
- `distinct_validity_trust_and_inapplicable_conditions_refuse`
- `procedure_capture_cannot_bypass_the_admission_transition`
- `interrupted_consolidation_rolls_back_before_fresh_comparison`
- `recorded_completion_replays_without_overwriting_later_memory`
- `captured_repository_evidence_is_rechecked_before_apply`
- `private_plan_and_preview_keep_bodies_off_stdout`
- `actual_comparison_retains_metadata_and_rejects_arrivals`
- `unavailable_unsafe_oversized_and_deleted_sources_never_prove_absence`
- `private_comparison_keeps_source_body_off_stdout`

Executing tests exercise stable identity, complete provenance, captured-session deduplication, revoked/deleted lineage refusal, history preservation, nested routing, source safety and size refusals, exact observation/envelope rules, conditions/scope semantics, conflict routing, deterministic UUID vectors, last-value union ordering and malformed input refusal. The ignored journeys retain actual source/CLI/private-output/recovery assertions rather than fake providers.

## Integration refresh

At Main's explicit instruction, copied every file listed by `git -C /opt/agent-memory-worktrees/rust-v015-int diff --name-only 8f69d726 fba4152d` byte-exact using `git ... show fba4152d:<path> > <path>`, then reapplied this lane's shared appends. The approved refresh includes checkpoint/inactive/retrieval/discovery/projections and their shared dependencies, tests, ledger and reports. These are baseline refreshes, not lane-owned edits.

`tests/vectors/security-regex-v1.json` SHA-256: `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41` (matches Main's required prefix).

## Declared deltas and foundation changes

No foundation changes. No intentional domain behavior changes. Foundation policies explicitly bound source/router and input reads and refuse non-UTF-8/BOM/unsupported deeply nested input rather than reproduce interpreter-dependent behavior. Version succession retains arbitrary-length positive decimal integers. No unsafe blocks, subprocesses or unbounded reads were added.

`grep -n 'unwrap()\|expect(\|\[.*\.\.' src/comparison.rs src/consolidation.rs` produced no matches (exit 1).

## Verification

Platform: `aarch64-apple-darwin`, Darwin 27.3.0 arm64; rustc 1.99.1 (`58cf8f9c3`, 2026-10-13), LLVM 23.1.1. Linux/cross-target verification was not run.

Observed commands/results:
- `cargo fmt --check`: passed.
- `cargo check --offline --all-targets --features dev-tools`: initially failed only absent retrieval/discovery, then exposed a digest API mismatch after refresh; repaired the imports and test helpers; final standalone check passed with zero warnings.
- `cargo test --offline --features dev-tools --test t2_consolidation`: initial registration fixture failures repaired using process-once provider registration; subsequent run passed, 13 passed / 11 ignored.
- `~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh - tests/t2_consolidation.rs`: marked 0 ported / 26 partial.
- Owned files formatted with `rustfmt --edition 2024`.

Final chained gate completed successfully (artifact `13260`, 615.02 seconds):
- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: clean, zero warnings.
- Two additional `cargo test --offline --features dev-tools --test t2_consolidation` runs: each 13 passed / 11 ignored. Together with the preceding successful standalone run, the target passed three consecutive times.
- `cargo test --offline --no-fail-fast --features dev-tools`: green on macOS, 1,074 passed across 69 suites; 141 ignored and 1,659 filtered as reported by the runner.

Implementation and available verification are complete. The only retained execution dependency is the explicitly ignored recovery-frontier integration coverage described above.

Coordination: `awa status --json` reported the repository managed; Main exclusively owns tracker/Git/GitHub writes. No tracker phase or reconciliation receipt is claimed by this lane. Vox status reported the harness node Ready with the worktree bound to its repository room.
