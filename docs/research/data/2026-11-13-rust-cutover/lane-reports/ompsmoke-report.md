# ompsmoke lane report

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-ompsmoke`, baseline detached `5764442c`. Baseline specification: `78eaf70` `scripts/evaluations/omp_native_smoke.py` and all seven methods of `NativeSmokeQualification` in `tests/contracts/test_omp_boundary.py`.

## Files

- New `src/bin/am_dev/omp_smoke.rs`.
- Modified `src/bin/am_dev/main.rs`: independent `omp-smoke` dispatch before release-provider registration.
- New `tests/c_test_omp_native_smoke.rs`.
- This external report. No Git, tracker, Python, crate, or foundation changes.

## Product implementation

`am-dev omp-smoke` ports argument parsing/help, receipt-owned command/extension validation, supported native-version gate, environment filtering, two exact human tasks, RPC protocol handling, explicit cancellation of interactive/unknown UI requests, redacted native diagnostics, exact sentinel counting, session-state checks, paginated public pending inventory, public installed status validation, finite attempt/outcome receipt validation, exact current-human public source verification, asynchronous read-only retry with a fixed deadline, final state/ownership rechecks, private report creation, summary JSON and success/failure exit codes. Both `qualified` and permanently false `full_native_qualification` are retained.

Every child uses `agent_memory::process`; native RPC uses owned descriptor-target Unix socket pairs, bounded nonblocking drains and no detached reader threads. Reads, directory inventories, pending rows and RPC frame/queue sizes have explicit limits. Evidence reads and report writes use filesystem foundations. Output directory creation is exclusive. Public source authority still comes from the real installed `am omp-boundary` verifier, not enrollment/status labels.

No product implementation branches are intentionally deferred. End-to-end native qualification itself is not claimed.

## Contract tests and gaps

The real installer helper is copied from `tests/t1_recovery_transactions.rs::installed_runtime` as requested. A second real install selects OMP. The controlled native binary is a shell launcher reexecuting this Rust test target's `controlled_native_protocol` worker; only OMP's protocol is controlled, never the installed `am`. Observer publication uses the real installed finite stream and checks actual READY/OBSERVED results and CLOSED inventory.

All seven original methods are written, including historical/copied-ID isolation, missing/incorrect current sources, unsupported versions before launch, completed-source root/digest tampering, unfinished shared registration, terminal stream receipts/current-leaf coverage and late duplicate/unsettled frames. They are unannotated while blocked:

`tests/contracts/test_omp_boundary.py::NativeSmokeQualification.` followed by:

- `test_historical_observers_cannot_qualify_new_current_session` — `#[ignore = "recovery_frontier"]`.
- `test_both_exact_current_human_turns_and_persistent_session_are_required` — `recovery_frontier`.
- `test_source_verifier_binds_completed_human_ids_text_roots_and_digests` — `recovery_frontier`.
- `test_source_verifier_waits_for_fault_free_shared_registration` — `recovery_frontier`.
- `test_stream_readback_requires_terminal_receipts_and_current_leaf_not_child_quiescence` — `recovery_frontier`.
- `test_late_native_frames_cannot_reuse_initial_answer_or_settled_proof` — `recovery_frontier`.
- `test_unsupported_binary_refuses_before_native_launch_or_human_turns` — `#[ignore = "pack surface: refresh/boundary (Admission)"]`, per Main's explicit instruction after the real install failed with `Command am boundary is not declared by the command surface`.

Supplemental enabled tests cover exact persistent human session/header/path/prompt/ID binding; canonical digest/content extraction; malformed/duplicate/explicit/dangling leaf selection; expired deadlines; command help; and the controlled worker entry point (which only activates in the protocol child). The seven ignored methods compile but were not exercised through their blocked runtime prerequisites. Main must remove ignores and annotate only after those prerequisites land and the methods pass; especially the two-version successful journey remains unverified here. No baseline obligation is falsely promoted.

## Annotations

Ran from this worktree:

`~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh - tests/c_test_omp_native_smoke.rs`

Result: `marked ported=0 partial=0`. All seven baseline methods are blocked and intentionally unannotated; supplemental helper tests do not pretend to discharge complete baseline obligations. The helper performed no obligation promotions.

## Declared deltas

- Per Main: only the Rust installer's `agent-memory/install-receipt-v3` is accepted. A v2 receipt refuses as `not a Rust runtime install`; no Python-era compatibility path.
- Observer route evidence names receipt-owned `am omp-boundary --stream`, replacing the baseline's Python hook argv.
- Help preserves all baseline option names, required arguments, model/approval descriptions and timeout default (240 seconds); executable name and receipt description reflect the Rust cutover, not argparse-specific wrapping.
- Explicit bounded strict UTF-8 file/JSON and directory/frame/queue limits replace baseline unbounded reads. Non-UTF-8 paths/text and excessive inputs refuse.
- Negative timeout values refuse as invalid arguments rather than entering Python subprocess/deadline edge behavior.
- Normal shutdown allows the baseline's 15-second natural-exit window. At its deadline the shared process foundation kills/reaps only the owned child, rather than the Python baseline's SIGTERM plus another 15-second wait. This follows the binding cutover process rule; normal success never cancels a naturally exited child.
- Errors are deterministic named failures rather than interpreter tracebacks; malformed prelaunch inputs still exit nonzero, while parser errors exit 2.

## Verification

Platform: `aarch64-apple-darwin`, Darwin `27.3.0 arm64`, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, LLVM 23.1.1. Linux/musl not run.

Final commands:

- `cargo fmt -- src/bin/am_dev/omp_smoke.rs src/bin/am_dev/main.rs tests/c_test_omp_native_smoke.rs` — applied.
- `cargo check --offline --all-targets --features dev-tools` — passed, zero warnings.
- `cargo test --offline --features dev-tools --test c_test_omp_native_smoke` — passed: 6 enabled, 7 ignored, 0 failed.
- `./target/debug/am-dev omp-smoke --help` — exited 0; complete expected option surface displayed (also asserted by enabled test).
- Required panic/slice grep on `src/bin/am_dev/omp_smoke.rs` — no `unwrap()`, `expect(` or indexing/slicing pattern matches.
- Ledger helper above — completed, 0 promotions.

Earlier compile errors (digest formatting/API signatures) and a helper-test JSON object-order assertion were corrected. The installed-runtime prerequisite failure was not suppressed or worked around: its complete baseline test is retained with Main's explicit ignore. No full suite or actual provider/native campaign was run.
