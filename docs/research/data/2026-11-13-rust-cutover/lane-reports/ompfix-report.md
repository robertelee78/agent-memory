# OMP delivery-edge fixes

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-ompfix`, starting at detached integration commit `06c186d`. No Git writes, Python execution, or subagents. `validate_receipt` was not edited; preserve the installation lane's v3 changes when integrating this file.

## Files changed

- `src/omp_installation.rs`
- `tests/t2_omp_install.rs`
- `src/migration_contract.rs`
- `tests/t1_migration.rs`
- `install.sh`
- `tests/t2_bootstrap.rs`

This report and retained verification evidence are outside the worktree in `/Users/robert.lee/.local/state/agent-memory/research/rust-cutover/`.

## Review items and covering tests

### P2-1 — saved binding reuse and Python-era adoption

`configuration` again reuses a saved current reader capability without the flag for a Rust-era receipt. Receipt schemas v1/v2 require fresh `--omp-peers-upgraded` at adoption. Historical capabilities retain the baseline upgrade-specific refusal unless the flag is supplied. Every returned binding is freshly produced by `binding`, including `rollout_attested: true`.

Covering integration tests:
- `rust_receipt_reuses_current_binding_without_renewed_attestation`
- `python_receipt_adoption_requires_renewed_attestation` (both v1 and v2, refusal and explicit attestation)
- `rust_receipt_with_stale_capability_requires_renewed_attestation`
- Existing `legacy_rollout_boolean_cannot_authorize_new_capability` remains intact.

The former test enforcing renewed attestation on every Rust reissue was replaced; no other existing assertion was weakened.

### P2-2 — baseline receipt gates

Restored the **1 MiB** receipt limit. The earlier lane report's claim that the baseline receipt read was unbounded was incorrect: `installation.py:101-105` already bounds it at `2**20` bytes. The previous Rust limit was 16 MiB.

The single private `read_receipt` now checks recognized schema, `verified is true`, string installation ID, array targets, object source, then returns `None` for `uninstalled: true` before native validation. Ordinary receipts additionally check each target's object/kind, normalized absolute path, and presence of `identity`. It retains baseline invalid-receipt, oversized-record, malformed-target, and relative-path refusal text. Reads remain descriptor-bound and trust checked.

This intentionally remains **one private receipt reader**, ready for the installation lane to replace with its shared reader; no second exported receipt API was introduced. Its outer schema gate includes v3, while changes to `validate_receipt` remain exclusively with the installation lane.

Private unit tests, requiring no activation provider:
- `baseline_uninstall_receipt_stops_before_native_validation_or_hashing`: retained OMP binding, `uninstalled: true`, empty `artifact_files`, absent owned targets/runtime -> `None`. The existing observer's immediate `None` branch produces `Native observer is not the current explicit owned binding`, before target verification or runtime hashing; this test directly covers the reader gate, not a registered-provider end-to-end observer invocation.
- `receipt_reader_enforces_one_mib_and_verified_before_native_validation`: rejects verified-false and 1.5 MiB receipts with exact messages, accepts exactly 1 MiB.
- `receipt_reader_checks_top_level_shape_and_each_target`: malformed top-level fields, every allowed kind, malformed targets, missing identity and exact relative-path refusal.

### P3-3 — first duplicate target wins

Removed `.rev()` from private `target`; observer/answer-first lookup now selects the first matching entry. `validate_receipt` and its separate last-wins semantics were not touched.

Covering unit test: `duplicate_observer_and_answer_first_targets_select_the_first_entry` distinguishes wrapper-first from copy-last records for the same path.

### P3-2 — nonfinite catalog text and readable-schema cycles

Mapped the codec's nonfinite refusal at the catalog decode site to `Nonfinite migration catalog`, leaving other decoder errors unchanged.

Covering integration tests:
- `nonfinite_catalog_constants_use_the_baseline_refusal`: NaN, Infinity and -Infinity with matching inventory hashes.
- `cycles_through_readable_schemas_are_refused`: readable v1 -> legacy -> middle -> readable v1, requiring the independent acyclic pass rather than route traversal's readable-target stopping rule.

### P3-4 — scratch assignment signal window

`am_workspace` ignores HUP/INT/TERM immediately around the `mktemp` assignment and immediately reinstates all three exit traps. POSIX sh and `set -eu` remain unchanged.

Covering integration tests:
- Existing `termination_while_the_binary_runs_still_removes_scratch` remains intact.
- Added adjacent `hangup_while_the_binary_runs_still_removes_scratch` and `interrupt_while_the_binary_runs_still_removes_scratch`.
- `signals_during_mktemp_cannot_leak_an_unassigned_scratch_directory`: real mktemp plus deterministic HUP/INT/TERM during command substitution, for each available test shell.

P2-3 was already fixed by the coordinator and was not touched. P3-1 (migration-plan runtime digest binding) is outside this assignment and was not changed.

## API, registration, obligations and deltas

No new public API, registration trait, CLI declaration, foundation change, crate, or obligation annotation. `MAX_RECEIPT`'s public value changes from 16 MiB to the baseline 1 MiB. No ledger update was needed: existing obligation annotations remain unchanged and the ledger suite passes. Added tests exercise private receipt/target helpers directly rather than registering a fake activation provider. No new activation-dependent test or fake provider was introduced; registered-provider observer/answer-first integration coverage remains dependent on the actual installation/harness implementation.

No additional compatibility delta beyond the requested Python-era renewed-attestation rule. Existing codec/foundation policies are unchanged.

## Verification

Platform actually exercised: `aarch64-apple-darwin`; Darwin 27.3.0, kernel `xnu-13432.60.57~99/RELEASE_ARM64_T6050`; rustc `1.99.1 (58cf8f9c3 2026-10-13)`, LLVM 23.1.1. No Linux or x86_64 build was run.

Commands and observed results:
- `cargo fmt --check`: final source passes.
- `cargo check --offline --all-targets --features dev-tools`: passes without warnings.
- `shellcheck --shell=sh install.sh`: passes.
- `dash -n install.sh`: passes.
- `for run in 1 2 3; do cargo test --offline --test t2_omp_install --test t2_bootstrap --test t1_migration; done`: three consecutive passing runs, respectively 20 / 29 / 11 tests per run.
- `cargo test --offline --no-fail-fast`: **495 passed, 0 failed, 2 existing ignored**. This whole-suite pass preceded the final narrow correction to retain the baseline relative-path refusal string.
- After that final correction, `cargo fmt --check` and `cargo test --offline --lib omp_installation::receipt_reader_tests`: **4 passed**, 0 failed.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/omp_installation.rs src/migration_contract.rs`: matches only in the `#[cfg(test)]` private test module; no production matches. Exact output retained in `ompfix-panic-patterns.txt`.

An initial private-unit run exposed two order-sensitive JSON-value assertions after sorted receipt encoding. These were corrected to compare complete sorted receipt bytes; the final four private tests pass. The initial async final-check job lost its tool completion record (reported through `xd://report_issue`); acceptance was rerun with durable log capture instead of claiming an unobserved result.

Retained evidence: `ompfix-acceptance.log` (complete three-run target and whole-suite transcript), `ompfix-acceptance.status` (0), `ompfix-final-reader-tests.log`, `ompfix-panic-patterns.txt`.

`awa status --json` reports the repository managed. Read-only issue searches did not uniquely map this delegated delivery-edge correction to a scoped managed story; no phase/health or tracker write was guessed, and no reconciliation receipt is claimed. Coordinator retains integration/tracker ownership.
