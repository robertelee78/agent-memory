# ompstream lane report — `omp_stream` (T2f)

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-ompstream`, detached at integration `681c3b7f`. No Git writes; coordinator merges the files below.

## Files

Created (owned):
- `src/omp_stream.rs` — constants, validators (`exact`, `uuid`, `integer`, `ids`, `identity`, `hint_identity`, `is_hint`), `NonlearningTailEvidence` + `nonlearning_tail_evidence`, `retain`, `directory`, bounded `entries`, `session_instances`, `session_creation_lock`, `preload`, `OmpTransport` registry.
- `src/omp_stream/frame.rs` — `encode_frame`, `Budget`, `FrameReader`, `Responder`, `ResponseWriter`, `Output`, `Sent`.
- `src/omp_stream/clock_window.rs` — `Platform`, `NativeImage`, `ClockWindows`, `VerifiedNativeDeliveryWindow`, `LateNativeDeliveryWindow`, `DeliveryWindow`, `stamp`.
- `src/omp_stream/linux_image.rs` — `LinuxNativeImage<P: ImageProbe>`, `ImageProbe`, `TimeNamespace`, `time_namespace_with`, `process_start`, cfg(linux) `/proc`+pidfd probe.
- `src/omp_stream/coverage.rs` — `StreamAuthority`, `FrameProof`, `Installed`, `OmpTransport` trait, `binding`, `Coverage` (open/attempt/refuse_invalid/started/outcome/transition/close), `source_refs`, `returned_refs`, `request_identity`, `HeldSource for SourceBarrier`.
- `src/omp_stream/executor.rs` — `FiniteExecutor`, `SourceCompletion`, `ObserveCall`, `ObserveError`, `Observer`, busy `metadata` retry.
- `src/omp_stream/inventory.rs` — `open_value`, `apply_identity`, `inspect`, `join_hints`, `incomplete_startup`, `inventory`, `drain_state`, `validate_drain`, `retire_for_recovery`, `resolve_recovery` (+ `RecoveryEvidence`).
- `src/omp_stream/wake.rs` — `wake_decision_directory`, seal evidence, root binding, `retain_wake_decision`, checked readback, `wake_decision_summary`, `wake_decisions`.
- `src/omp_stream/run.rs` — `Timing`, `StreamStartup`, `RunOptions`, `run`, `main`, `STREAM_PARSER`.
- `tests/t2_omp_stream.rs` — 93 tests (91 annotated obligations + 2 byte-exact frame tests).

Shared files (append-only):
- `src/lib.rs`: `pub mod omp_stream;` (alphabetical).
- `src/process.rs` (foundation, appended at end): `set_nonblocking`, `write_some`, `wait_writable` (nonblocking response pipe); cfg(linux) `pub mod parent` { `Stat`, `parent_pid`, `own_pid`, `pidfd_open`, `pidfd_exited`, `read_proc`, `stat`, `open_image`, `fstat`, `read_block` }.
- `src/host.rs` (foundation, appended): `time_ns()` (wall-clock ns for `opened_at`; pre-epoch refused).
- `tests/wiring-pending.txt`: `omp_stream::register_omp_transport waits for omp_transport`.
- `tests/obligations/python-baseline-v1.json`: marked via am-dev (77 ported, 14 partial; id lists beside this report: `ompstream-ported.txt`, `ompstream-partial.txt`).

Python, TypeScript, schemas untouched.

## Baseline coverage (every function of `omp_stream.py`)

`_exact _uuid _installation_id _integer _digest _sha _ids identity _hint_identity _is_hint _NonlearningTailEvidence _nonlearning_tail_evidence _retain _base _directory _entries _session_instances _session_creation_lock preload encode_frame _Budget FrameReader ResponseWriter(send/_loss_notice/_write/finish) _stamp _linux_process_start _linux_time_namespace _LinuxNativeImage(observe/observed/valid/close) VerifiedNativeDeliveryWindow(support/reserve) ClockWindows(control/window/revoke/end) _LateNativeDeliveryWindow _SourceCompletion(validate/source_result/commit_source/_write_outcome/retry_busy_outcome/before_reserve/wake_failure/close) _binding _attest_binding _verify_binding Coverage(all methods) _source_refs _returned_refs _request_identity FiniteExecutor(metadata/refuse_invalid/execute/_execute) _wake_decision_directory _seal_evidence _wake_root _retain_wake_decision _checked_wake_decision _wake_decision_summary wake_decisions _open_value _apply_identity _inspect _join_hints _incomplete_startup _inventory inventory drain_state validate_drain retire_for_recovery resolve_recovery run main` — all ported. `_FrozenPackedImports` has no Rust counterpart (see deltas). No unported branch is known.

## Public API / seams

- `StreamAuthority` (one coherent environment seam): `base`, `source_barrier`, `observation_barrier`, `attest`, `verify(… held, body(context))`, `transport`. Production `Installed::new(runtime_root)`: `bank_writes::{bank_path,state_paths}`, `turn_sources::source_barrier`, `private_state::observation_barrier`, `omp_installation::verified_observer_binding` (attest) and `omp_installation::observe_observer_binding` (verify; marker context `{"root","bank","project_id","scope":{checkout_key},"held"}` agreed with OmpBoundary), registered transport. All public stream operations take `&dyn StreamAuthority`.
- `OmpTransport` (seam for unported `omp_transport`, the baseline's three lazy imports): `observe`, `drain_after_response`, `native_disposition_ids`. `register_omp_transport` / crate `omp_transport_provider()`; missing registration = `Kind::MissingProvider` "OmpTransport provider is not registered; finite sources remain pending". Future implementer: `omp_transport` lane. Listed in wiring-pending.
- `StreamStartup` (seam for `omp_boundary::stream_startup`): passed as a parameter to `omp_stream::main(argv, prog, startup) -> CommandResult` (no registry). OmpBoundary's `OmpStream` trait (`drain_state`, `main`) will be implemented by an adapter at merge: `drain_state(&Installed::new(rt), repo, Some(base), read_only)`, `main(argv, prog, &adapter)`.
- `omp_installation` used directly; its `OmpActivation`/`HeldSource` reused: `impl HeldSource for turn_sources::SourceBarrier` lives in `omp_stream/coverage.rs` (OmpBoundary informed, must not duplicate).
- `private_state::observation_barrier`, `scope_binding::fresh_enrollment_locations`, `turn_sources::*` used directly.
- CLI: no `table.rs` entry — `am omp-boundary --stream` is owned by omp_boundary's dispatch (streaming route, cli-report recipe exception); `main` declares its args with `cli::parser` (`--repo --session --instance --lifecycle --artifact --omp-agent-dir|--agent-dir`, all required).

## Clock relation (design rule)

Fields ported exactly: CLOCK `{nonce,j0,j_deadline}`, CLOCK-REPLY `{nonce,p1,p2}` (decimal strings), CLOCK-ACK `{nonce,j0,j3}`, CANCEL `{nonce}`; L = p2 − j3, U = p1 − j0, L ≤ U, ≤950 ms interval, MARGIN 100 ms, Darwin ceiling drift 164/10000. Local stamps come only from `clock::monotonic_ns` (Linux `CLOCK_MONOTONIC`, macOS `mach_absolute_time`·timebase — the CPython relation); never `Instant`. The CPython interpreter/version/`get_clock_info` gate is replaced by `clock::clock_identity() == "linux:clock_gettime(CLOCK_MONOTONIC)"` before any Linux image observation. Linux windows are usable only with the unchanged two-image allowlist, matched time namespaces, live same-start parent via pidfd and unchanged image (E = 0). Darwin keeps `provisional-unqualified`. READY always says `wake_support: unqualified`; any clock fault only revokes guidance — source capture, coverage and drain never depend on it. Note for the omp.ts retarget: the parent of the Rust worker must be the omp process itself (direct `spawn(binding.executable, …)`), otherwise the Linux image check (parent = qualified image) refuses windows — which is safe (no wake), never weaker.

## Wire / on-disk parity

Frame header `AM249 <KIND> <seq> <len> <nonce|->\n` + sorted compact UTF-8 JSON (`turn_sources::encoded`), matching hooks/omp.ts parser (header ≤256 ASCII, body ≤64 KiB responses, `instance_id` first then fields). Records: open/attempt/started/outcome/transition/close/retirement/recovery/transition-recovery/startup-incomplete/wake-decision with exact schemas, field sets and `O_EXCL` 0600 + fsync + dir fsync + readback (via turn_sources). Locks: flock `stream.lock`, shared `live.lock`, `omp-stream-session-locks/<sha>.lock`, source barrier. Byte-exact frame tests included.

## Declared deltas

1. `StreamAuthority` makes the baseline's monkeypatched seams (`_base`, `source_barrier`, `_verify_binding`, `_attest_binding`, `omp_transport.*`) explicit; contracts implement it with a real descriptor-level `source_barrier_at` and counters. No global registry is populated by tests.
2. `preload`: Rust links its closure statically; only the "cannot adopt another executable closure" refusal is kept (per-process artifact digest). `_FrozenPackedImports` has no equivalent; FrozenImports obligation stays unported.
3. Python object-identity/mint/`__reduce__` guards (windows, evidence, completion, proofs) are structural: private fields, no Clone/serialization, `compile_fail` doctest on `VerifiedNativeDeliveryWindow`; `SourceCompletion::validate` uses pointer identity of the request.
4. Python `MemoryError`/`RecursionError` fatal policy for the optional wake writer: Rust has no such recoverable classes; all optional-writer failures are swallowed (OOM aborts the process).
5. `ObserveError::{Contract, Unexpected}` replaces Python exception classes (ContractError ⇒ observer-refused before start; anything else ⇒ unknown).
6. Unbounded directory scans now bounded by `MAX_ENTRIES = 65536` ("Finite stream inventory exceeds bound"); sequence file names beyond u64 refused.
7. I/O-level failures inside `inventory` surface their own refusal text instead of the generic "Finite stream inventory is missing, offline or malformed" (all remain refusals).
8. `close_live` releases the live lock without an exit recheck (Rust Drop).
9. The response writer uses a nonblocking dup of stdout with 25 ms `poll` waits (foundation), exactly the baseline `select` loop; finish grace 2×100 ms without joining a blocked thread.
10. `opened_at` uses `host::time_ns` (refuses a pre-epoch clock).

## Obligations

Ported (77): all 9 Framing, 50 of the 52 CoverageFixture (the other 2 partial), 5 LinuxImageControls (driven through `ImageProbe` controls; interpreter cases become clock-identity cases), 13 DeadlineWindows.

Partial (14):
- CoverageFixture `test_real_io_failure_does_not_manufacture_invalid_request_refusal`: real failure injected at outcome publication (occupied `outcomes`) instead of after the attempt write; same property (accepted attempt kept, no refusal manufactured).
- CoverageFixture `test_startup_typed_busy_retries_before_open_and_ready`: preload→startup event order not observable (preload not injectable); busy retry, backoff and no-OPEN-before-success asserted.
- 12 WakeDecisions (`offered_binds…`, `not_armed…`, `empty_verified…`, `offered_writer_checks…`, `spoofed…`, `canonical_parsed…`, `malformed…`, `nonseal…`, `conflicting…`, `substituted_oversize…`, `false_observed_send…`, `bounded_summary…`): exercised with a test observer that writes the root-wake record; real transport root/cursor/request semantics and `omp_installation::diagnostics`/doctor readback await omp_transport + harness composition.

Deferred / unported (not annotated):
- CanonicalWriterContention (24) and MetadataBarrierBoundaries (2): need real `omp_boundary` (`_activate_marker`, `verify_marker`, `_observe_marker`, `stream_startup`), installer receipt and checkpoint fixtures.
- CanonicalWriterObservationBoundaries (14): exercise `local_registry.exclusive` via syscall-level interposition; not omp_stream code, belong with local_registry.
- WakeDecisions `test_existing_root_retains_legacy_bare_pending…`, `test_retained_root_after_cursor_failure…`, `test_optional_writer_failure_after_response…`, `test_failed_observed_send…` (need omp_transport); `test_optional_writer_memory_error…`, `test_optional_writer_recursion_error…` (delta 4).
- AnswerFirstQualification (1): installation + diagnostics.
- FrozenImports (1): delta 2.

## Commands and results

Platform: aarch64-apple-darwin, Darwin 27.3.0 arm64, rustc 1.99.1 (58cf8f9c3 2026-10-13). Linux targets compile-checked only.

- `cargo fmt --check` → clean (owned files formatted with `rustfmt --edition 2024`).
- `cargo check --offline --all-targets --features dev-tools` → 0 warnings, 0 errors.
- `cargo check --offline --features dev-tools --lib --target x86_64-unknown-linux-musl` and `--target aarch64-unknown-linux-musl` → clean.
- `cargo test --offline --features dev-tools --test t2_omp_stream` ×3 → 93 passed, 0 failed, 0 ignored each run.
- `cargo test --offline --features dev-tools --test wiring` → 2 passed, 1 ignored. `--test obligations_ledger` → 5 passed.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/omp_stream.rs src/omp_stream/*.rs` → no matches (also none in the appended foundation code).
- Full suite `cargo test --offline --no-fail-fast` (aarch64-apple-darwin): exit 0; 31 top-level harnesses (incl. doc-tests) → 691 passed, 0 failed, 35 ignored (pre-existing ignores; nested re-exec child summaries not counted). The `VerifiedNativeDeliveryWindow` compile_fail doctest passes. Log: `/tmp/ompstream-full.log`.

Process notes (honesty): one empty `python3 -` heredoc was invoked by mistake (no code, no inspection); two import-line fixes were made with `sed` before switching back to the edit tool. Neither affects content.
