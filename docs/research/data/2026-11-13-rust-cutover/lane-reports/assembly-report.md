# assembly lane report

## Delivery

The two complete baseline modules are ported: `assembly.py` → `src/assembly.rs`; `recovery_transactions.py` → `src/recovery_transactions.rs`. No baseline function or runtime branch is intentionally omitted. No Python was executed, no new crates were added, and no Git/GitHub writes were made.

Native routes are `am context` (`preview`, `check`) and `am transaction` (`preview`, `apply`, `recover`), in installation.MEMORY_ROUTES baseline order. Transaction intentionally calls the existing bank_transactions module, matching the baseline bank_transactions.py entry helper.

## Lane-owned changes

- src/assembly.rs
- src/recovery_transactions.rs
- src/cli/helpers/assembly.rs
- src/cli/helpers/bank_transactions.rs
- src/cli/helpers/mod.rs
- src/cli/table.rs
- src/cli/mod.rs
- src/lib.rs
- src/bank_writes.rs
- tests/t2_assembly.rs
- tests/t1_recovery_transactions.rs
- tests/wiring-pending.txt
- tests/obligations/python-baseline-v1.json

Shared declarations and trait methods were appended without reordering prior declarations. The appended module declarations carry rustfmt::skip to retain that append-only contract. The context/transaction route entries are inserted at their mandated baseline positions.

The worktree started at c8c7feb5. Main authorized dependency refresh to 5bd11582: all 58 paths changed between those integration revisions were read with git show and copied byte-exact, then lane-owned shared additions were reapplied. Those imported dependency changes are not separate lane implementations. The path list is retained in tool artifact 13551.

## APIs and wiring

Assembly exposes request_value, unit_key, labeled, normal_units, context_text, assemble, check, read_input, command_result and the validated ConflictGroup result. It calls real retrieval, comparison, bank_contract and bank_writes modules directly; recovery scope checks use the existing bank_writes::RecoveryFrontier provider.

The adapter currently has a local `recovery_transactions::runtime_root()` locator with the same pack-grandparent meaning as layout_recovery's CLI. Main explicitly approved this local expression pending its shared `cli::runtime_root()` addition; at integration the two adapter call sites can use that shared locator and the local helper can be deleted. Main also owns the corresponding admission/consolidation corrections.

Recovery transactions exposes marker_for, source_check, validate, credited_rows, resolution_plan, planned_resolution, terminalize, apply_transaction, legal_partial, rollback_plan, retired_nested, retained_coverage_review, nested_preview, recover_nested, nested_receipt and apply_checkpoint. Runtime-root-sensitive operations take an explicit runtime path. Its RecoveryTransactions adapter implements all four bank_writes trait methods, including the nested_receipt/recover_nested seams introduced by the layout_recovery lane. register_providers registers this adapter; the waits-for-recovery_transactions pending line is removed.

One required method was appended to bank_writes::RecoveryFrontier: observe_index(repo, owner, transaction, indexed, runtime_root), corresponding exactly to recovery_frontier._observe_index during projected ledger-head/index-gap inspection. The eventual recovery_frontier port must implement it. There is no default/fake implementation.

Held transaction validation directly invokes admission, governance, consolidation and checkpoint. Durable journals, write/transaction receipts, deterministic resolution UUIDs, rollback-plan boundaries, independent coverage reviews, sentinel/marker retirement linkage, ordered file replacements and injected fault points are retained. All private reads have explicit bounds, and filesystem trust/locking/writes use the existing foundation and domain APIs.

## Tests and obligations

`tests/t2_assembly.rs` contains all seven baseline AssemblyContract test bodies, plus independent active request/merge, sorted UTF-8 context golden, normal-unit deduplication/diversity/origin, and bounded input/refusal tests.

`tests/t1_recovery_transactions.rs` contains active ordered-prefix/rollback-plan/checkpoint-receipt/transaction-route tests and ten complete public-flow test bodies covering the six NestedReconciliationContract methods and four LayoutCheckpointFaults methods. These include actual child error/exit injection, retained originals, interrupted rollback plans, exact canonical receipt/head gaps, competing writer preview, changed completion/foreign safeguards/external edits and checkpoint receipt identity.

Seventeen public-flow tests are explicitly ignored with `needs recovery_frontier registered`, because the real provider is not ported. No stand-in provider was installed. Their obligations remain partial until those real flows can run. The remaining nested fault-matrix/legacy-runtime/closure suites are not claimed as ported by this lane. Native pure-function tests are additional coverage, not substitutes for the deferred public-flow obligations.

## Declared deltas and foundation changes

No additional semantic or command-surface deltas. The release-wide strict UTF-8, surrogate/depth, bounded-reader and native-help policies apply. No foundation changes or unsafe code.

## Platform and checks

macOS aarch64-apple-darwin; Darwin 27.3.0 (xnu-13432.60.57~99/RELEASE_ARM64_T6050); rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1.

Commands executed:

- rustfmt --edition 2024 on lane-owned Rust source/test/helper files.
- cargo fmt --check.
- cargo check --offline --all-targets --features dev-tools.
- cargo test --offline --features dev-tools --test t2_assembly --test t1_recovery_transactions: three consecutive passes on final code; assembly 4 passed/7 ignored, recovery_transactions 4 passed/10 ignored each run.
- ~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh - tests/t2_assembly.rs tests/t1_recovery_transactions.rs.
- cargo test --offline --no-fail-fast --features dev-tools.
- grep -n 'unwrap()\|expect(\|\[.*\.\.' src/assembly.rs src/recovery_transactions.rs: no matches.
- rustc -vV; uname -a.

Initial API-check failures (missing helper help fields, digest signature, validate_files argument, and then the absent governance dependency) were corrected. An active receipt fixture assertion was corrected to compare encoded JSON objects rather than insertion-order-sensitive Value equality. Final cargo fmt --check and all-target check passed with zero warnings. Both targets passed three consecutive times. The ledger marker recorded 17 partial obligations on its first final run and zero additional changes on its idempotent repeat (ledger totals: partial 543, ported 332, unported 987). The complete macOS cargo test --offline --no-fail-fast --features dev-tools suite passed after the runtime-root correction, including obligations, wiring and doc tests; no failures. Final combined command completed successfully in 605.83 seconds; full evidence is artifact://13669. A prior full-suite run also passed (artifact://13639).

The deferred held public-flow fixture installs the actual built native binary through public development-source installation, uses its receipt-owned executable and measured runtime digest, and derives private state from the world's explicit AGENT_MEMORY_STATE_HOME. It does not manufacture an installation receipt or a recovery provider.

## Tracking authority

awa status --json reported managed=true for robertelee78/agent-memory. Main is explicitly the sole Git/GitHub writer and owns story/project reconciliation; this worker did not advance tracker state. Vox status reported the bound room Ready.
