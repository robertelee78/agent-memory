# Lane `layoutcore` report

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-layoutcore`, detached snapshot of integration `46a0100`. No Git writes, no sub-agents, no new crates. Platform: macOS arm64 (`Darwin 27.3.0 arm64`, `aarch64-apple-darwin`), `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. Linux is not run by this lane.

## Files

Created:
- `src/layout_resolution.rs` — port of `layout_resolution.py` (610 lines), plus the single definition of the value/path/lock helpers the six modules share.
- `src/layout_units.rs` — `layout_units.py` (202).
- `src/layout_restore.rs` — `layout_restore.py` (370) + the `LayoutRecovery` seam.
- `src/layout_recovery_context.rs` — `layout_recovery_context.py` (141).
- `src/layout_recovery_temps.rs` — `layout_recovery_temps.py` (681).
- `src/recovery_quiescence.rs` — `recovery_quiescence.py` (136) + production `RecoveryQuiescence` provider.
- `tests/t2_layout_core.rs` — 13 module tests on real private files.
- `tests/t2_layout_core_probes.rs` — 29 tests: probe journal/pin/cleanup/ambiguity with real fault injection (`AGENT_MEMORY_FAULT_STAGE`/`MODE`, token-bound `/tmp/agent-memory-test-*` world, owned child process via `agent_memory::process`, 120 s deadline).

Modified (all append-only):
- `src/lib.rs` — six `pub mod` lines.
- `src/bank_writes.rs` — appended `RecoveryFrontier` methods (below) and `pub const HOLD / NESTED / TRANSACTION / SENTINEL` after the trait (Main's decision: single definition; recovery_frontier must reuse them).
- `src/private_state.rs` — appended `RecoveryQuiescence::validate` with exactly Main's posted signature.
- `src/bank_state.rs` — appended `ComparisonView::invalidate_frontier_transition(&self, repo) -> Result<String>` and `safe_text(&self, value, maximum) -> Result<()>`; renamed the private getter to `pub(crate) comparison_view_provider` and migrated its one caller (line 338). This is byte-identical in intent to the frontier lane's approved edit; expect a trivial merge (same lines) — keep one copy.
- `src/error.rs` — appended `Kind::ProbeOwnership { path }` and `pub(crate) fn probe_ownership(path)` (message `pending-ambiguous-probe-ownership`; baseline `ProbeOwnershipError.probe_path`).
- `src/host.rs` — appended `effective_gid()` and `random_bytes(&mut [u8])`.
- `src/fs/descriptor.rs` — appended `impl Descriptor { pub fn make_directory_at(name, mode) }` (raw `mkdirat`, umask applies, inode never chmodded — baseline `os.mkdir(name, mode, dir_fd=…)`; the existing `create_directory` chmods, which would hide the metadata the probe must observe).
- `src/cli/mod.rs` — `register_providers` registers `recovery_quiescence::RecoveryQuiescence`.
- `tests/wiring-pending.txt` — deleted `private_state::register_recovery_quiescence waits for recovery_quiescence`; added `layout_restore::register_layout_recovery waits for layout_recovery`.
- `tests/obligations/python-baseline-v1.json` — 29 obligations `unported → partial` (via `am-dev obligations mark` in this worktree).

## Public API

Every entry point takes an explicit `runtime_root: &Path` (as `layout_recovery_state::paths`); there is no ambient runtime fallback.

- `layout_resolution`: `atomic_marker`, `interrupted_successor`, `advance_ledger`, `restore_hold`, `retire`, `transaction_evidence`, `claims_for`, `make_output_plan`, `output_plan_context`, `check_output_plan`, `evidence_fields`, `resolution_receipt`, `resolution_completion`, `require_completed_resolutions`, `resolution_units`, `resolve`, `accumulated_coverage`; consts `OUTPUT_PLAN`, `OUTPUT_PLAN_FIELDS`, `SENTINEL_FILE`, `MAX_LISTING`, `MALFORMED`.
- `layout_units`: `SCHEMA`, `FIELDS`, `merged`, `revised`, `map_at`, `preview`, `apply`.
- `layout_restore`: `SCHEMA`, `FIELDS`, `RECOVERY_JOURNAL`, `RECOVERY_RECEIPT`, `Operation`, `RecoverySource`, trait `LayoutRecovery`, `register_layout_recovery`, `value`, `boundary`, `preview`, `temporaries`, `metadata_probes`, `recover`, `apply`.
- `layout_recovery_context`: `runtime_guard`, `evidence`, `RecoveryReadSet::{observe, check}` (sealed: private fields, no other constructor, not serializable).
- `layout_recovery_temps`: `source_name`, `entries`, `clean`, `probe_plan`, `validate_probe_plan`, `probe_entries`, `probe_clean`, `probe_complete`, `probe`.
- `recovery_quiescence`: `ATTESTATION_STATEMENT`, `runtime_binary`, `measured_runtime`, `frontier_capable`, `attestation_value`, `selected_attestation`, `selected_at` (baseline `_selected_attestation`), `validate`, provider struct `RecoveryQuiescence`.

Reverse edges for the frontier seam (Main wires at integration): `layout_resolution::{advance_ledger, restore_hold, retire, resolve}` and `recovery_quiescence::{validate, selected_at}` (also reachable via the registered `private_state::RecoveryQuiescence`).

## Seams

- `bank_writes::RecoveryFrontier` (implementer: recovery_frontier) — appended, no defaults: `require_scope_owned` (Admission's exact declaration, included so the snapshot compiles), `marker_bytes(bank, name, expected)`, `marker_value(raw)`, `invalidate(repo) -> Result<String>`, `range_value`, `verified_hold_value(repo, owner, transaction, runtime_root) -> Result<Option<scope_binding::VerifiedEnrollmentHold>>` (reuses the existing hold type: same fields as baseline `VerifiedHold`), `pending_rows`, `successor_token(token, ids, operation_id, generation_change)`, `reservation`, `claim_value`, `resolve_reference(repo, reference, runtime_root)`, `validate_comparison_review(repo, proof, owner, transaction, runtime_root) -> Result<Option<Value>>`. No trait implementers or fixtures exist in this snapshot, so no fixture edits were needed.
- `private_state::RecoveryQuiescence` — implemented and wired (`selected_attestation`, `validate`).
- `bank_state::ComparisonView` (implementer: comparison/retrieval, T2a) — `safe_text` used by resolution/quiescence.
- `layout_restore::LayoutRecovery` (implementer: layout_recovery) — every baseline `layout_recovery.*` call of these modules: `fields`, `independent`, `result`, `operation`, `recovery_source`, `receipt_value`, `recover`, `hold`, `view`, `expected_result`, `ledger_value`, `manifest_view`, `rebind`, `atomic_marker`, `new_directories`, `replace_source` (takes `&RecoveryReadSet`), `eligibility`, `proposal_value`, `partial_boundary`, `unit_map_value`, `pending_rows`. Unregistered → `Kind::MissingProvider` refusal "LayoutRecovery provider is not registered" (tested).
- Existing `scope_binding::require` and `turn_sources` canonical writer barrier (`bank_writes::turn_sources_provider`) are used directly.

## CLI

None: these modules have no `main`; their routes are `bank recover-layout resolve|revise-units|rollback|recover` owned by layout_recovery's CLI.

## Obligations

Partial (29), probe-ownership half ported at the `layout_recovery_temps` boundary with real fault injection on the baseline fixture shape (router, original record, record moved into a new nested directory, that directory → the baseline's four reachable probe roles, asserted); missing: the public `am bank recover-layout apply/recover/status/retention` half (result JSON `failure_code`/`probe_path`, transaction marker, canonical/source/Git invariants), which needs layout_recovery + recovery_frontier:
- `test_layout_metadata_probe_faults.py::LayoutMetadataProbeFaults.` `test_attempt_journal_{error,exit}`, `test_directories_removed_{error,exit}`, `test_created_{0..3}_{error,exit}`, `test_removed_{0..3}_{error,exit}`, `test_unpinned_{file,directory}_remains_pending_until_operator_removes_it`, `test_before_pin_{file,directory}_error_cleans_owned_probe`, `test_foreign_replacement_after_pin_remains_pending`, `test_changed_nonce_after_pin_remains_pending`.
- `test_layout_metadata_probe_retention.py::LayoutMetadataProbeRetention.` `test_unknown_pin_refuses_without_deletion`, `test_changed_attempt_plan_refuses_without_deletion`, `test_foreign_probe_at_completed_path_refuses_without_deletion` (retention CLI half needs layout_recovery_retention).

Ids file: `~/.local/state/agent-memory/research/rust-cutover/ledger-marks/layoutcore-new-partial.txt`. Marked first with `am-dev obligations mark --status partial --ids-file …` in this worktree (the original script targeted the mid-merge integration tree), then the updated `ledger-mark.sh - tests/t2_layout_core.rs tests/t2_layout_core_probes.rs` was re-run from this worktree: `marked ported=0 partial=0` (already marked); the 29 ids read back as `partial`. Ledger totals in this worktree: 182 ported, 226 partial, 1454 unported.

Deferred (not annotated; no compiling public API exists to drive them, same rule as layoutobs):
- `test_layout_metadata_probe_faults.py`: `test_journal_{error,exit}` (stage in layout_restore/layout_recovery apply), `test_late_{created,removed}_{error,exit}` (`layout-recovery-before-replace` stage), `test_fresh_inspection_refuses_unowned_probe_without_mutating_sources`, `test_changed_inode_flags_after_pin_remain_pending`, `test_changed_pin_flags_with_unchanged_inode_remain_pending` (need `chflags`/`FS_IOC_SETFLAGS` fixture helper; module logic ported — `inode_flags` is pinned and compared), `test_legacy_stale_probe_without_marker` (v0.0.14 runtime fixture) — wait for layout_recovery, recovery_frontier.
- `test_layout_metadata_probe_retention.py`: `test_reviewed_cleanup_retains_completed_attempts_and_historical_receipt`, `test_missing_attempt_completion_refuses_without_deletion` — wait for layout_recovery_retention.
- `test_layout_resolution_faults.py` (5) and `test_layout_revision_faults.py` (4) — wait for layout_recovery, recovery_frontier (fixtures `small_pending`, `discard_proposal` and the CLI).
- Quiescence journeys in `test_mixed_private_capabilities.py` (`test_layout_first_native_activation_…`, `test_omp_first_mixed_pending_…`, `test_completed_omp_drains_…`, `test_private_lifecycle_observation_refuses_…`) and `test_layout_recovery_runtime_rollback.py::HeldRuntimeRollbackContract.test_interrupted_capable_update_rolls_back_exact_runtime_and_keeps_hold` — wait for recovery_frontier, layout_recovery, omp_boundary/installation.

Unannotated module tests (t2_layout_core.rs): probe-plan name derivation (expected names recomputed from the baseline label rule), probe success/completion evidence and 0700 dirs, unreproducible metadata → `pending-metadata-preservation` with probes removed, unknown attempt/pin evidence preserved, exact journal temporaries (source and marker names, digest/uid/gid/mode match, partial temporary refused, `clean`), child-restore `boundary` prefix/ordering/external-edit/reverse-plan, child metadata probe plan, `LayoutRecovery` MissingProvider refusals, output plans (review/coverage excluded from the digest, rebuilt-bytes and future-time refusals), `evidence_fields`/receipt/completion shapes, `atomic_marker` handoff and refusal without temporaries, quiescence `selected_at` proposal/renewal-head/corrupt-head, attestation capability refusal.

Not ported baseline branches: none omitted from the six modules. Every function above is complete; the ones that call recovery_frontier, layout_recovery or comparison refuse with MissingProvider until those land.

## Declared deltas

- Baseline KeyError/TypeError on malformed private evidence → typed refusal `Recovery evidence is malformed or incomplete; preserve it for inspection` (`layout_resolution::MALFORMED`) or the nearest baseline message.
- Private directory listings (`resolution-journals`, `nested`, probe attempts/pins, attempt children) are bounded by `MAX_LISTING = raw::MAX_ENTRIES` (131072); baseline `iterdir()` was unbounded.
- Measured runtime = `artifact::inspect_runtime(runtime_root/skills/agent-memory, runtime_root/bin/am)` (`installation::RELEASE_PACK/RELEASE_BINARY`) for baseline `artifact.inspect(artifact.PACK)`; executing binary is the runtime root's `bin/am`.
- `layout_units.merged` validates every range with `range_value` before sorting (baseline sorted first; ill-typed ranges raised TypeError).
- `layout_restore.apply` finish-mode directory removal: a symlink at a reviewed directory path refuses (baseline `exists()` skipped a dangling symlink, then failed readback).
- `quiescence.validate` with `runtime_digest = None` publishes under the JSON key `"null"` (what `json.dumps` produced for a `None` key).
- `require_completed_resolutions`: a non-UTF-8 journal name refuses as unknown evidence.

## Foundation changes

`Kind::ProbeOwnership` + constructor; `host::effective_gid`, `host::random_bytes`; `Descriptor::make_directory_at` (all append-only). Integration `e6d1348` has `host::random_32()`: both read the same private `entropy()`; to keep one primitive, `random_32` should delegate to `random_bytes` (or my nonce call can switch to `random_32`) at merge.

## Process notes

- Two accidental trivial interpreter invocations happened during this session (an empty `python3 - <<EOF` probe and `python3 -c 1`, both no-ops, never touching baseline code), and one harness eval cell used Python to paste an edit body. No baseline Python was executed or inspected by execution.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' ` over the six modules: two hits, `layout_recovery_temps.rs` `&digest[..16]` / `&digest[..20]` on a SHA-256 hex digest (always 64 ASCII chars; commented). `host::random_bytes` slices `&block[..count]` with `count ≤ 16` from `chunks_mut(16)` (commented).

## Commands (macOS arm64)

- `cargo fmt --check` — clean.
- `cargo check --offline --all-targets --features dev-tools` — 0 warnings, 0 errors.
- `cargo test --offline --test t2_layout_core --test t2_layout_core_probes` ×3 — 13 passed and 29 passed each run.
- `cargo run -q --offline --features dev-tools --bin am-dev -- obligations mark --status partial --ids-file …/ledger-marks/layoutcore-new-partial.txt` — 29 marked.
- `cargo test --offline --no-fail-fast --features dev-tools` — exit 0: 818 passed, 0 failed, 38 ignored (511.6 s). Run on the `46a0100` snapshot plus this lane; not refreshed to `a725b97` (this lane did not ask for security/recall_catalog/omp_stream and calls none of them directly).
