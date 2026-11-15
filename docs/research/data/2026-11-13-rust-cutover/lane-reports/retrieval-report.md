# Retrieval lane

## Implementation

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-retrieval`.

Ported the complete production behavior of `retrieval.py`, `discovery.py`, and `projections.py`: qualification before selectors/ranking, complete supersession inventory including withheld replacements, exact ID/path and Unicode ordinary-text matching, complete-result budgets, observed checkout conditions, source-bound semantic navigation and qualification privacy, optional authenticated catalog fallback/publication, private pending-recovery output, and disposable canonical-reference projections. No Python was executed and no Git/GitHub writes were performed.

### Lane changes

- New: `src/retrieval.rs`, `src/discovery.rs`, `src/projections.rs`.
- New: `src/cli/helpers/retrieval.rs`, `src/cli/helpers/discovery.rs`, `src/cli/helpers/projections.rs`.
- New: `tests/t2_retrieval.rs` (34 tests; the temporary included test fragment was merged into this file and removed).
- Shared appends/wiring: `src/lib.rs`, `src/error.rs`, `src/bank_writes.rs`, `src/cli/mod.rs`, `src/cli/helpers/mod.rs`, `src/cli/table.rs`, `tests/wiring-pending.txt`.
- Approved foundation append: `src/host.rs`, `current_executable() -> Result<PathBuf>` with constant refusal text; the regression checks the consumer-visible absolute-path contract. Runtime identity uses the existing receipt-qualified `cli::pack_root()` plus this executing-binary locator, without a guessed fallback.
- Ledger: `tests/obligations/python-baseline-v1.json`.
- Report: this file.

Exact prerequisite-refresh paths (collect lane changes above, not these stale copies):

```text
Cargo.lock
Cargo.toml
docs/research/data/2026-11-13-rust-cutover/lane-reports/ompstream-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/recallcatalog-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/security-report.md
src/bank_state.rs
src/bank_transactions.rs
src/cli/helpers/mod.rs
src/cli/helpers/security.rs
src/cli/mod.rs
src/cli/table.rs
src/error.rs
src/host.rs
src/lib.rs
src/model_reference.rs
src/omp_stream.rs
src/omp_stream/clock_window.rs
src/omp_stream/coverage.rs
src/omp_stream/executor.rs
src/omp_stream/frame.rs
src/omp_stream/inventory.rs
src/omp_stream/linux_image.rs
src/omp_stream/run.rs
src/omp_stream/wake.rs
src/process.rs
src/recall_catalog.rs
src/security.rs
tests/obligations/python-baseline-v1.json
tests/t1_a1.rs
tests/t1_bank_transactions.rs
tests/t1_layout_history_workspace.rs
tests/t1_layout_recovery_state.rs
tests/t1_private_state.rs
tests/t1_project_identity.rs
tests/t1_scope_binding.rs
tests/t1_security.rs
tests/t2_layout_recovery_observation.rs
tests/t2_omp_stream.rs
tests/t2_recall_catalog.rs
tests/t_support_world.rs
tests/vectors/security-regex-v1.json
tests/wiring-pending.txt
```

Approved prerequisite refreshes are not lane-authored implementation changes: all changed files in `46a0100..e6d1348` and `e6d1348..a725b97` were copied from those exact integration commits, then lane appends were restored. This includes Cargo.toml/Cargo.lock; host/process/recall_catalog/omp_stream and its submodules; native security and its helper; model_reference; shared CLI/error/lib files; the corresponding integration tests, vectors, ledger and existing lane reports. `src/bank_transactions.rs` was also refreshed from e6d1348 as explicitly requested. `src/bank_state.rs` alone was refreshed from d1b2766 for the expanded ComparisonView contract. These refreshes must not replace newer integration work during collection.

## Public API and composition

- `retrieval::{request_value, git_context, observed_context, scope_reasons, matching, evaluate_sources, retrieve, command_result}`. `retrieve(repo: &Path, request: &Value, owned_lock: Option<&[u8]>, deferred_paths: &[String]) -> Result<Value>`.
- `discovery::{read_input, bounded_text, trace_value, bounded_source, check_trace, navigation_observation, start, navigation_tables, node, qualified_read, command_result}`, plus typed `Navigation`.
- `projections::{value, read_input, build, check, command_result}`.
- `Kind::DiscoveryFailure { code, repair }` retains actionable baseline discovery refusals; generic CLI refusals withhold unsafe input/source details.
- Native routes: `search`, `get`, `discover start/node/read`, `projection build/check`.
- `retrieval::ComparisonView` implements the existing bank_state seam and is registered in the CLI composition root. Its observed-context method is native retrieval; catalog invalidation calls recall_catalog directly; safe-text qualification delegates to the unported comparison provider.
- Reused existing `bank_writes::RecoveryFrontier` and `bank_contract::RecoveryCoverage` registrations. Appended the coordinator-agreed `RecoveryFrontier::require_scope_owned(repo, project, expected, owner, transaction)` signature immediately after require_scope; retrieval/check_trace pass the owned writer marker and `None` transaction.
- New seam `discovery::Comparison::{inspect, safe_text}`; `register_comparison` waits for `comparison` and its accessor refuses with MissingProvider.
- New seam `projections::InactiveProjection::filter_projection`; `register_inactive_projection` waits for `inactive` and its accessor refuses with MissingProvider.
- No duplicate security or recall-catalog seam: security is called directly after the approved a725b97 refresh; catalog is called directly using its native runtime/generation/catalog interfaces.

## Obligations

33 exact baseline obligation IDs are annotated in `tests/t2_retrieval.rs`: all 13 RetrievalContract methods, five retrieval-facing DiscoveryContract methods, all seven DiscoveryWorkflowContract methods, and all eight ProjectionContract methods.

- Fully ported/marked ported: `ProjectionContract.test_bounded_and_deeply_malformed_input_refuses_without_echo_or_canonical_change`.
- Other 32 are marked partial, not falsely claimed executed: their real-process/filesystem test bodies compile but remain explicitly ignored until their real recovery_frontier and, as applicable, checkpoint, comparison, inactive or transaction CLI providers are registered. No fake provider was installed.
- Nine tests run now: exact/Unicode selectors; request bounds; projection shape/version references; malformed bounded projection input; failure envelopes; bounded discovery trace validation; real canonical-observation invalidation/security/conflict filtering; real lifecycle/condition/type and complete-result budgets; the absolute native executable locator. The library also tests order-insensitive Python dictionary equality without integer precision loss or array-order loss.
- The three DiscoveryContract methods centered on admission mutation/review (`test_meaningful_admission_creates_native_topics_preserves_payloads_and_replays`, `test_navigation_without_trace_collision_or_substituted_proposal_refuses`, `test_discovered_equivalent_cannot_be_omitted_from_comparison_to_admit_duplicate`) are deferred to admission/comparison ownership and were not annotated or ledger-marked here.
- Marking used the required ledger-mark.sh. The first script version wrote integration; after Main changed it to worktree-local, it was rerun here. Final lane totals are one ported and 32 partial annotations.

## Declared deltas and safety

- Foundation codec deltas are inherited: strict UTF-8, refusal of lone surrogate escapes and depth >256; no Python decoder/interpreter-version accidents are reproduced.
- Explicit input documents are bounded before decode and their canonicalized paths are read through held descriptors. Canonical source reads retain the 16,384-byte bound; requests/traces and retrieval result budgets retain 65,536 bytes; projection representations retain 2 MiB.
- Dictionary comparisons ignore object-key order (and retain Python numeric equality), while arrays retain order. This preserves acceptance of reordered caller JSON rather than depending on serialization order.
- No new unsafe code, crates, executable scripts, interpreter probes, retries or deadline widening. Git uses identity_git's existing finite child deadlines. Retrieval elapsed time uses the foundation monotonic clock.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/retrieval.rs src/discovery.rs src/projections.rs` returned no matches.
- No unlisted production functions or branches were omitted; unresolved cross-module calls use the declared typed seams.

## Verification

Platform: macOS arm64, `aarch64-apple-darwin`; Darwin 27.3.0, xnu-13432.60.57~99/RELEASE_ARM64_T6050. `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, LLVM 23.1.1. Linux targets were not run.

Observed commands/results:

- `rustfmt --edition 2024` on the lane-owned Rust modules/helpers/test file, then `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: clean, zero warnings. An initial prerequisite-only failure (missing omp_stream and host::random_32) was resolved by the approved integration refresh; helper route type errors and an unused shared-fixture re-export warning were also fixed.
- `cargo test --offline --features dev-tools --test t2_retrieval`, final three consecutive runs: each 9 passed, 25 ignored, zero failures. The focused library equality test also passed.
- `cargo test --offline --features dev-tools --test obligations_ledger --test wiring`: 5 ledger and 2 wiring tests passed; the existing all-ports-complete wiring gate remains ignored. The earlier include-fragment annotation visibility failure was fixed by moving all annotated tests into the top-level target.
- Final `cargo test --offline --no-fail-fast --features dev-tools`: 908 passed across 60 suites, 74 ignored; completed in 517.38 seconds (artifact://12699). The preceding complete run also passed (907 tests before the final locator regression; artifact://12639). An earlier attempt had a 300-second command deadline and timed out; complete runs used a finite 3600-second command deadline without changing any child-process deadline.

All assigned finish-line commands passed on macOS. Unregistered-module end-to-end tests remain explicitly ignored as required; their 32 obligations are partial, not counted as executed parity proof. Ready for coordinator collection and provider integration.

## Integration refresh requested before merge

Refreshed every non-owned path in the exact set `git -C /opt/agent-memory-worktrees/rust-v015-int diff --name-only a725b97 8f69d726`, byte-for-byte from `git show 8f69d726:<path>`, and restored lane appends. The full 68-path refresh list is retained in artifact://12886. This supersedes the earlier d1b2766-only ComparisonView refresh. No Git rebase/commit/write was performed.

The refreshed `tests/vectors/security-regex-v1.json` and the integration object both hash to `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41`.

Additional approved changes:

- Added native `projections::Projections` implementing `diagnostics::Projections::{max_bytes,value}`, registered it in the composition root, and removed the diagnostics pending line.
- Replaced admission's discovery provider call with `crate::discovery::start(repo, task, "comparison", &empty_context)`, matching baseline discovery's default context. Deleted admission's Discovery trait/registry/accessor/registration and pending line; no compatibility shim remains.
- Migrated `installation/providers.rs` RuntimePackRoot to `host::current_executable()`. Receipt-qualified pack location and actual executing-binary identity are retained.
- Retained exactly the integration copy of `RecoveryFrontier::require_scope_owned`; no duplicate lane declaration was reapplied.
- Restored the baseline public-route sequence: developer, identity, init, bank, search, discover, get, capture, admission, omp-boundary, private, sharing, exchange, projection, writes, migrate. There is only one private route. Checkpoint is not present in this integration commit.
- Inactive is absent at 8f69d726, so its projection provider remains a typed pending registration.
- Added a native diagnostics projection-provider regression; `t2_retrieval` now has 35 tests.

Re-marked this worktree's ledger: one ported and 32 partial obligations. On 8f69d726, fmt is clean, all-target/dev-tools check has zero warnings, t2_retrieval passed three consecutive runs (10 passed, 25 explicitly ignored), and ledger/wiring tests passed (5 + 2, existing final-cutover wiring gate ignored). Final `cargo test --offline --no-fail-fast --features dev-tools` passed: 1045 tests across 69 suites, 119 ignored, 765.33 seconds (artifact://13082). No checkpoint merge hash was delivered before completion; inactive remains pending at the requested 8f69d726 base.
