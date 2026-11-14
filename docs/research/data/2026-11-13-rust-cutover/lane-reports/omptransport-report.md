# omptransport lane report — `omp_projection`, `omp_control`, `omp_transport` (T2f)

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-omptransport`, detached at integration `d1b2766`. No Git writes, no Python executed, no sub-agents, no new crates. Read: lane brief, cutover design, ompstream/ompboundary/ompinstall lane reports, baseline `omp_projection.py` (432), `omp_control.py` (60), `omp_transport.py` (1,735) completely, and the relevant parts of `omp_installation.py`, `turn_sources.py`, `identity_git.py`, `tests/contracts/test_omp_*.py`.

## Files

Created (owned):
- `src/omp_projection.rs` — every baseline function: `_instruction_uri` → `instruction_uri`, `_instruction_relative` → `instruction_relative`, `stored_timestamp` (→ `StoredTime`), `checked_view`, `_persistence_truncated`, `_safe_tree`, `_text`, `_blocks`, `project`, `_valid_instruction_context` → `valid_instruction_context`, `_project_owned_manual` → `project_owned_manual`. Constants `MAX_SOURCE`, `EXCLUSIONS`, `ROLES`, `FORBIDDEN_KEYS`, `PERSISTENCE_TRUNCATION_NOTICE`, `MAX_INVENTORY`.
- `src/omp_control.rs` — `pure_batch`, plus `shell_split` (exact POSIX `shlex.split` port: whitespace split, quotes, escape rules, `ValueError` → `None`).
- `src/omp_transport.rs` — core records: `digest`, `canonical_uuid`, `uuid5`/`uuid5_url` (RFC 4122 v5 via `sha1`), `exact`, `admission_metadata`, `read`/`read_optional`, `retain`, `replace_cursor`, `session_dir`, `directories`, `cursor_value`, `validate_root_wakes`, `segments`, `new_segment`, `source_bound_origin`, `active_segment`, `writable_successor`, `files` (`Names::{Hex64,Dash36}`), `snapshot_path`, `native_disposition_ids`, `_enroll`, `gap`, `retained_gap`, `capture` (+ `ManualRead`), `matched_metadata`, `enroll_sealed`, `seal`, `enroll_admission`, `_shared`, `_shared_inventory`, `all_segments`, `registration_lookup`, `registration`, `drain_enrollments`, `coverage`, `manifest_value`. `Site` (repo, runtime root, optional held enrollment context) and `Segment`/`Seg` (shared mutable cursor: the baseline mutates one cursor dict through several aliases).
- `src/omp_transport/inventory.rs` — `historical_completed_family`, `_segment_enrollments`, `segment_inventory`, `pending_inventory`, `list_pending`, `source`/`_source`, `complete`, `transport_guard` (fresh locations + shared source barrier + exclusive `omp-transport.lock`, rechecked at exit).
- `src/omp_transport/observe.rs` — `wake`, `resolve_tool_result`, `_checked_payload_coverage` → `checked_payload_coverage`, `_drain_after_response`, `observe` (request validation, guard/lock order, marker scope, retire/resolve recovery fences, ingest/seal sweep, pure-control classification, owned-manual reads, seal/terminal sealing, exit checks, completion commit, wake). `ObserverScope` trait and `Observation` call record.
- `src/omp_transport/providers.rs` — `Transport` (one provider of both transport seams), `InstalledScope` (production observer scope), `Harness` (T2f half of the activation interface).
- `tests/t2_omp_transport.rs` — 18 tests (17 active, 1 ignored with reason).
- This report.

Shared/non-owned files (append-only or Main-approved):
- `src/lib.rs`: `pub mod omp_control;`, `pub mod omp_projection;`, `pub mod omp_transport;` (alphabetical).
- `src/security.rs` (Main-approved, appended at end): `pub fn matches_any(text: &str) -> Result<bool>` — every SECRETS + SUSPICIOUS pattern searched once over unredacted text using the existing `PATTERNS` and `find_valid` (no second pattern list). The 16 MiB bound is named at the omp_projection call sites (`MAX_SOURCE`).
- `src/turn_sources.rs` (Main-approved): `fn enrollment_namespace` → `pub(crate) fn enrollment_namespace` (visibility only).
- `src/cli/mod.rs` `register_providers` (appended at end): one `omp_transport::Transport::installed()` registered to `omp_boundary::register_omp_transport` and `omp_stream::register_omp_transport`; `omp_installation::register_activation(InstallationActivation { env: Environment::process(), services: Services::registered(), executable: current_exe(), harness: omp_transport::Harness })` through `wired(...)` (skipped only when the running executable path is unresolvable, which leaves the typed `MissingActivation` refusal).
- `tests/wiring-pending.txt`: removed `omp_installation::register_activation …`, `omp_stream::register_omp_transport …`, `omp_boundary::register_omp_transport …` (all wired).
- `tests/t2_omp_boundary.rs` (Main-approved, one assertion + comment, lines 1134-1144): `--stream` without an installed receipt now refuses at the registered activation's `locations()` ("No installed Rust runtime receipt owns this executable; pass --pack PATH") instead of "OMP activation interface is not registered…".
- `tests/obligations/python-baseline-v1.json`: marked via `ledger-mark.sh` (below).

## Seams

- `omp_boundary::OmpTransport` and `omp_stream::OmpTransport` (two traits in two non-owned modules, same callee) are both implemented by the single `omp_transport::Transport`; not collapsed into one trait because both traits are frozen in non-owned modules with different method sets (boundary: inventory/list/source/complete with explicit runtime root; stream: observe/drain/dispositions without one). `Transport::installed()` resolves the runtime root through `cli::pack_root()`; `Transport::at(root)` takes it explicitly.
- `omp_installation::register_activation`: composed from installation's `InstallationActivation` (locations, verify_targets, activation_template, activation) and `omp_transport::Harness` (implements `installation::OmpHarness`: `diagnostics` = read-only locks + `list_pending` + stream inventory + wake decisions with unavailable-summary fallback + existing-only barrier/`verify_marker`; `observe_marker` = borrow (recheck) or acquire the shared source barrier, `omp_boundary::observe_marker_operation`, recheck at exit; `instruction_uri` = `omp_projection::instruction_uri`).
- No new seams: every callee is ported and called directly.

## CLI

No new route: `am omp-boundary` (owned by omp_boundary) reaches this module through the registered seams (`--list-pending`, `--source`, `--stdin` completion, `--stream`).

## Obligations

Ported (marked `ported`, 9):
- `test_omp_projection.py::StoredProjection.{test_retains_tool_call_and_result_attribution, test_reasoning_and_provider_fields_not_copied, test_opaque_or_secret_source_never_retained, test_tool_payload_metadata_is_not_copied}`.
- `test_omp_boundary.py::StoredBatchControls.test_foreign_companions_and_human_input_never_become_own_maintenance`.
- `test_omp_stored_authority_v2.py::NativeStoredAuthorityV2.{test_changed_header_branch_and_parent_graph_refuse_before_transaction, test_retired_view_closed_identity_negatives_refuse_before_source_writes, test_nested_foreign_repository_cwd_cannot_borrow_parent_bank_scope, test_unavailable_or_nongit_manager_cwd_refuses_before_source_start}` — the refusals happen in request validation before any barrier, so the fixture is a Git repository with a plain bank and an observer scope that fails the test if reached (baseline: initialized bank + marker scope, never reached either); `on_started` is asserted unrun and no segment exists afterwards.

Additional unannotated tests: captured CPython security vectors for `matches_any` (all 31 per-pattern cases, including the plain-safe negative and positives); projection gap/carrier/execution shapes; instruction URI/timestamp/closed instruction-context; reserved boundary invocation classification and `shlex` cases; UUIDv5 against the CPython documentation vector, cursor/admission validators; unsupported-request refusals before start; retained capture → gap → seal → enrollment → sealed-gap resolution with exact digests/IDs; unbound/foreign families and interrupted-record faults; installed real-binary route (ignored, below).

Deferred, not annotated (bodies not written; each needs the private enrollment context, i.e. `recovery_frontier` registered — `scope_binding::private_enrollment_context` refuses with MissingProvider — and most also an initialized bank fixture (`initialization`) or a real native install (`private_state` hooks)):
- `test_omp_boundary.py::NativeSources.*` (11 besides the protected-root one owned by ompboundary), `OwnedInstructionBoundary.*` (20), `ExplicitNativeOwnership.*`, `NativeLifecyclePublic.*`, `NativeSmokeQualification.*`.
- `test_omp_stored_authority_v2.py` remaining 33 methods (observe/seal/wake/complete through the marker scope).
- `test_omp_segment_inventory_batch.py` (16), `test_omp_enrollment_progress.py` (27), `test_omp_source_phase.py` (53), `test_omp_writer_fence.py` (3), `test_omp_finite_outcome_fence.py` (2), `test_omp_wake_decisions.py` (7 unported; the 12 partial belong to ompstream), `test_omp_stored_source_lifecycle.py` (4), `test_omp_finite_stream.py` transport-dependent remainder (39).
- `installed_stream_passes_observer_binding_and_reaches_real_transport` (real built `am`, development-source install with `--presets omp --omp-peers-upgraded`; installed `am omp-boundary --stream` outside a bank → `INACTIVE inactive-no-bank`, exit 0, i.e. the observer binding verified and the finite worker started; forged artifact refused; installed `am omp-boundary --list-pending` in a bank → real transport inventory, verified/empty/complete) is `#[ignore = "needs private_state hooks registered …"]`: the real install refuses with "Private-state hooks provider is not registered" (reproduced manually in a scratch world with this build). In a bank, `--stream` itself additionally needs `recovery_frontier` (stream startup mints the private enrollment context), so source frames reaching `observe` cannot be shown end-to-end until then.

## Implementation completeness

Every baseline function and branch of the three modules is ported. Paths that reach `scope_binding::private_enrollment_context` (observer scope, `turn_sources::inspect`/`register`/`private_namespace`, `omp_boundary::verify_marker`) refuse with the provider refusal until `recovery_frontier` is registered; this is the shared T1 dependency, not a gap in this lane.

## Declared deltas

1. Observer context: `omp_installation::observe_observer_binding` hands its callback a context *value* (`{root,bank,project_id,scope,held}`, frozen interface). The transport needs the live `PrivateEnrollmentContext`, so `InstalledScope` mints it inside the same held marker/binding scope with `scope_binding::private_enrollment_context`, requires an identical context value, and runs the body under `private_enrollment_reads` of that context (baseline: the yielded object itself). Every enrollment-context check applies to the minted context; the outer marker scope still rechecks at exit.
2. Test seam: the baseline contracts monkeypatched `_verify_observer_receipt`; Rust makes it the explicit `ObserverScope` parameter of `observe` (production: `InstalledScope`).
3. `observe` errors are all `ObserveError::Contract` (Rust has no separate OSError/TypeError classes); busy barrier/lock refusals keep their typed kind so the stream still retries them.
4. Exception-class distinctions collapse: places that caught only `(OSError, ValueError)` (segment_inventory row lookups, registration) now also turn baseline TypeError/KeyError into the same row fault/refusal.
5. `persisted=` of `project` was unused in the baseline and is not carried; `capture` keeps it.
6. Path existence checks use no-follow `lexists` where the baseline used `Path.exists()`: a dangling link counts as present and its later read refuses.
7. Previously unbounded scans are bounded: sibling segments in `seal` and segment record names (65536), pending index (`MAX_TURNS`, same refusal text), all-sessions listing (65536 via bounded listing instead of read-then-count).
8. `seal` reads captured snapshots through the validated `snapshots/<sha256>.json` reference (baseline joined the raw `snapshot_ref`); retained snapshots carry no `ordinal`, so the baseline `(ordinal or 0, native)` sort key is `native`.
9. `stored_timestamp` accepts ASCII digits only (Python `\d` matched any Unicode decimal digit before `fromisoformat`).
10. Python value equality (`1 == 1.0 == True`) is replaced by canonical-encoding equality for retained JSON comparisons; for the owned-manual carrier this is equivalent because the baseline separately refused bool/float stand-ins.
11. A truthy non-string `toolCallId` is a key that never names a retained invocation (baseline dict lookups with an int/bool key); a list/object `toolCallId` refuses (baseline TypeError).
12. `event_ordinal` must fit `u64` (Python ints are unbounded).
13. Guidance text and inventory JSON reproduce the baseline `json.dumps` styles (ASCII default for the repository path and faults, compact ASCII insertion order for inline rows); page/inline byte bounds use the canonical sorted compact length, computed incrementally (same values).
14. `HeldSource` cannot be downcast, so the borrowed barrier is rechecked at entry and exit and the transport lock path comes from `omp_boundary::base(repo)` (baseline: the held lock's own parent directory).

## Foundation changes

None (the security and turn_sources edits above are Main-approved domain additions, not foundation).

## Panic scan

`grep -n 'unwrap()\|expect(\|\[.*\.\.' src/omp_projection.rs src/omp_control.rs src/omp_transport.rs src/omp_transport/*.rs` → no matches. RefCell cursors use `try_borrow`/`try_borrow_mut` (refusal, never panic). The `matches_any` append has none either.

## Commands (platform: aarch64-apple-darwin, Darwin 27.3.0 arm64, rustc 1.99.1 (58cf8f9c3 2026-10-13); Linux musl targets not built here — no new libc symbols, no `unsafe`)

- `cargo fmt --check` → clean.
- `cargo check --offline --all-targets --features dev-tools` → 0 warnings, 0 errors.
- `cargo test --offline --features dev-tools --test t2_omp_transport` ×3 → 17 passed, 0 failed, 1 ignored each run.
- `~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh /tmp/omptx-ported.txt tests/t2_omp_transport.rs` (names file = the 9 annotated ids) → `marked ported=9 partial=0`; ledger now `partial: 266 ported: 271 unported: 1325`.
- `cargo test --offline --features dev-tools --test obligations_ledger --test wiring` → 5 passed; 2 passed, 1 ignored.
- Whole suite `cargo test --offline --no-fail-fast --features dev-tools` (after the ledger mark): cargo exit 0; 63 `test result` lines, 977 passed, 0 failed, 50 ignored (log `/tmp/omptransport-full2.log`). The first full run, before the ledger mark, had exactly one failure (`obligations_ledger::live_ledger_is_consistent`: the 9 newly annotated ids were still `unported`) and 976 passed.
- Panic scan as above → no matches.
- Manual reproduction of the install gate (scratch world under `/private/tmp/am-omptx.*`, this build's `am`, `am install --development-source … --presets omp --omp-agent-dir … --omp-peers-upgraded`) → `am: Private-state hooks provider is not registered`.

## Hand-over for Main

- When the private-state hooks hash lands: refresh, drop the `#[ignore]` on `installed_stream_passes_observer_binding_and_reaches_real_transport`, run it.
- When `recovery_frontier` registers: the deferred transport obligations above become portable; `observe` source frames reach the transport end-to-end.
