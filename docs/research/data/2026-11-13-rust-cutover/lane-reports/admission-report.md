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

## Checkpoint 2 — memory refresh implementation

The refresh implementation is in `src/memory_refresh.rs`, with the public `am refresh` and `am boundary` routes in `src/cli/helpers/memory_refresh.rs` and contract coverage in `tests/t2_memory_refresh.rs`. It calls integrated retrieval directly, reuses the existing semantic-sharing registration, and calls `sharing_frontier::{inspect, request_value}` directly; no second frontier or comparison seam was introduced.

Implemented paths cover protected transaction inventory and retained-journal refusal, typed/sanitized current diagnostics, local-only diagnostic prerequisites, canonical/index/HEAD witnesses, receipt-bound disposable cursors, legacy and v2 inspection, complete-source preparation, exact prepared-envelope validation, source checking, historical apply retry, cursor advancement, private output preflight, and the transaction validator. `turn_check::register_memory_refresh` is wired in the production registry and its pending line removed.

Main approved two required methods appended to the existing `sharing_hold_mapping::SemanticSharing`: `inspect(repo, base)` and `snapshots(repo, base, quarantine_records)`. Their production adapter belongs to the Sharing lane. `Kind` additionally carries structured refresh-current failures, prepared-envelope refusal, and untracked-canonical-memory refusal.

The tests use real isolated Git repositories, local bare remotes, linked worktrees, private runtime state, and production registrations. Journeys requiring unported `recovery_frontier`, `semantic_sharing`, or `recovery_transactions` retain explicit dependency ignores; no synthetic provider grants semantic authority. Active coverage exercises transaction path policy/races, retained evidence, fixed diagnostic text, envelope shape/tampering, bounded v2 input, relative CLI output preflight, mode-preserving private directory creation, identical private-output retry, and complete-source security scanning. The ancestor-rebinding creation control checks the resulting pinned chain rather than injecting a race inside creation; its original obligation remains partial. The large-source security action-bound obligation already has full active coverage in integrated `t1_security`; this lane adds the complete-source primitive control without claiming that obligation anew.

Approved non-owned dependencies were refreshed byte-exact from `cc85cc37`, then from `c8c7feb5`; local append-only registrations/routes were reapplied. The security vector fixture remains SHA256 `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41`.

The first checkpoint-2 `cargo fmt --check` passed. The following all-targets check stopped at the two direct unresolved `sharing_frontier` calls; the Sharing module is not integrated yet. Per Main's final instruction, no provisional `sharing_frontier.rs` or `semantic_sharing.rs` was copied. Checkpoint-2 finish-line verification and ledger marking await the approved Sharing merge hash; the checkpoint-1 results above do not establish checkpoint-2 verification.

## Checkpoint 2 — integrated handover

### Source and integration boundary

Main's approved final dependency base is `5764442c`. Every changed non-owned path from `c8c7feb5..5764442c` was refreshed with a shell `git show 5764442c:<path> > <path>` redirect, and only the lane's append-only declarations, routes, structured errors and registrations were reapplied. This supersedes the earlier waiting state. The final Sharing files are integrated dependencies, not provisional worktree copies:

- `src/sharing_frontier.rs`: SHA256 `7c0230f22cf9556307988bbd9a50ae1eae93fc9c2fdd5228bc5ed15855fd4234`.
- `src/semantic_sharing.rs`: SHA256 `c96fdfe91cb54cf08b6c3f8cce91d9346ef0cbef4c3985f628ae5e797103ec2d`.
- `src/sharing_page_export.rs`: SHA256 `d8e0226e0cd49f29ca7f7a66437db4919141ee130dea60827f0ca6cad97941df`.
- `tests/vectors/security-regex-v1.json`: SHA256 remains `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41`.

Checkpoint-2 authored/extended files, beyond the integrated dependency refresh:

- `src/memory_refresh.rs` — new domain port.
- `src/cli/helpers/memory_refresh.rs` — new typed native parser/handler.
- `src/lib.rs`, `src/cli/helpers/mod.rs`, `src/cli/table.rs`, `src/cli/mod.rs` — module declarations, `refresh`/`boundary` routes, set-once diagnostic provider and transaction validator.
- `src/error.rs` — structured refresh refusals and constructors.
- `tests/wiring-pending.txt` — remove the wired memory-refresh pending line.
- `tests/t2_memory_refresh.rs` — new contract target, including real installation-backed output preflight.
- `tests/t2_admission.rs` — remove obsolete comparison/discovery names from dependency-ignore reasons; recovery-frontier dependency remains.
- `tests/support/installed_runtime.rs` — Main-approved extraction of the existing real native-install fixture.
- `tests/support/mod.rs` — shared fixture declaration.
- `tests/t1_recovery_transactions.rs` — replace its private duplicate install fixture with the shared definition.
- `tests/obligations/python-baseline-v1.json` — mark only qualified lane obligations.

The SemanticSharing `inspect`/`snapshots` trait extension is present in the refreshed Sharing implementation, so this checkpoint does not separately overwrite its integrated adapter. No foundation implementation was changed.

### Public API and provider composition

Public domain entry points: `transaction_inventory`, `transaction_inventory_observed`, `state_operations`, `local_prerequisites`, `result`, `index_hash`, `cursor_path`, `cursor`, `inspect`, `current`, `current_with_runtime`, `current_result`, `current_result_from`, `validate_transaction`, `prepare`, `save_cursor`, `prepared_proposal`, `apply`, `check_sources`, `read_input`, and `command_result`. Supporting public types are `CurrentFailure`, `PolicyPin`, `MutablePin`, `InventoryStatus`, `TransactionInventory`, `CursorObservation`, and `MemoryRefresh`; constants name the bounds, cursor schema/fields and fixed repair messages.

`current_with_runtime(repo, runtime_root)` is the single quick-current implementation, explicitly approved by Main. `current(repo)` resolves `cli::runtime_root()` and delegates. Hold-store/runtime inspection and private-output guards use the runtime root, not the nested pack root. The direct-API race worker receives the real runtime root returned by the installation fixture; it does not fake `PackRoot`.

`MemoryRefresh` implements the existing `turn_check::MemoryRefresh` and `bank_transactions::TransactionValidator` seams. Production registration uses a checked `LazyLock` around the set-once diagnostic registry, and registers transaction operation `refresh`. No new caller-provider seam was introduced. Private-state composite hooks remain outside this lane; checkpoint 1's public `admission::read_input(path, maximum)` remains available for their composition.

The shared native-install fixture copies the real shipped pack and compiled command to its isolated development runtime, runs the real native `install --development-source ... --mode link --presets none --no-guidance`, then returns the validated receipt's runtime root and source binary. Refresh journeys invoke that returned installed command. No handwritten installation receipt or fake authority provider was added.

### Coverage and ledger qualifications

Final repeated target outcomes (three consecutive runs of the same three-target command):

- `t2_admission`: 10 passed, 6 ignored.
- `t2_memory_refresh`: 20 passed, 32 ignored.
- `t1_recovery_transactions` after fixture extraction: 4 passed, 10 ignored.

All remaining lane integration ignores now name only `recovery_frontier`, which is not ported/registered in this dependency base. This includes complete v1/v2 refresh inspection/import/recall, exact source/proposal rejection, conditions, conflicts/dependencies, tombstones, linked worktrees, Git-merge/cursor recovery, receipt witnesses, stale/offline authority, replay/rollback, final-recheck API and CLI races, runtime path/umask diagnostics, hidden records/hold corruption, and recall-catalog equivalence. Those test bodies are ported and compile, but their behavior is not claimed as executed. The ordinary worker tests return without entering the race unless launched by their gated parent; those ordinary returns do not qualify the race obligations.

The active unsafe-output contract now uses a real installed binary, real bank bytes, staged source/index state, feature HEAD and unrelated work, and checks all state after every semantic/publish/refresh refusal. It is no longer ignored. The baseline treats an absent `git_metadata_writes_performed` field as false; the Rust assertion uses the same optional-field semantics.

Fully qualified active obligation families: all seven `TransactionPathPolicy` methods; relative public refresh output preflight; preserving private directory modes/links; identical private-output retry; unsafe public semantic/publish/refresh output refusal. The existing integrated `t1_security` complete-refresh action-bound obligation was already ported and was not claimed anew.

`ledger-mark.sh /tmp/admission-refresh-ported-names.txt tests/t2_memory_refresh.rs` marked **9 newly ported, 33 newly partial** obligations. The already-partial unsafe-output obligation, now fully active and passing, was promoted separately with `am-dev obligations mark --status ported --ids-file /tmp/admission-refresh-completed-ids.txt`. Other pre-existing ported statuses were preserved. The directory-creation ancestor-rebinding obligation remains partial: the Rust control qualifies the held chain after rebinding, not a deterministic injected race inside directory creation. All dependency-ignored journey obligations remain partial.

### Verification evidence and corrections

- Final `cargo fmt --check`: clean.
- Final `cargo check --offline --all-targets --features dev-tools`: succeeds with zero warnings.
- `cargo test --offline --features dev-tools --test t2_admission --test t2_memory_refresh --test t1_recovery_transactions`: final three consecutive runs green as counted above.
- Required panic-pattern scan across the three domain files and both CLI helpers: `grep -n 'unwrap()\|expect(\|\[.*\.\.' ...` produced no matches.
- First active refresh run found a fixture-only capacity-code typo (`bank-source-bytes` versus the established `bank-source-byte-capacity`); the expectation was corrected.
- Ungating the output contract exposed the missing real installation fixture, then an overly strict optional JSON-field assertion; both were corrected to the baseline setup/semantics, without weakening any baseline assertion.
- A subsequent setup attempt invoked identity enrollment before unported recovery-frontier registration. The active preflight control now constructs real canonical/staged feature state without enrollment, so it still proves output refusal precedes semantic/identity work. Integration journeys retain their honest recovery-frontier ignores.
- No Python, Git writes, GitHub writes, new crates, or unsafe code were used by this checkpoint. No baseline function/branch is intentionally omitted from the domain port. Remaining unverified runtime behaviors are the dependency-ignored journeys and the explicitly partial directory-creation race qualification above.

### Exact final non-owned refresh inventory

The byte-exact `c8c7feb5..5764442c` dependency refresh contained these 87 paths (local lane appends are separately listed above):

```text
Cargo.toml
docs/research/data/2026-11-13-rust-cutover/lane-reports/assembly-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/boundaries-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/governance-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/hooks-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/layoutrecovery-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/manifestbound-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/omptransport-fix-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/reapfix-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/routes-report.md
docs/research/data/2026-11-13-rust-cutover/lane-reports/sharing-report.md
skills/agent-memory/hooks/omp.ts
skills/agent-memory/hooks/opencode-v2-queue.js
src/admission.rs
src/assembly.rs
src/bank_writes.rs
src/bin/am_dev/obligations.rs
src/bin/am_dev/release.rs
src/cache_recovery.rs
src/claude_boundary.rs
src/cli/helpers/admission.rs
src/cli/helpers/assembly.rs
src/cli/helpers/bank_contract.rs
src/cli/helpers/bank_state.rs
src/cli/helpers/bank_transactions.rs
src/cli/helpers/claude_boundary.rs
src/cli/helpers/codex_boundary.rs
src/cli/helpers/governance.rs
src/cli/helpers/hooks.rs
src/cli/helpers/layout_recovery.rs
src/cli/helpers/mod.rs
src/cli/helpers/project_identity.rs
src/cli/helpers/semantic_sharing.rs
src/cli/helpers/tombstones.rs
src/cli/mod.rs
src/cli/table.rs
src/codex_boundary.rs
src/consolidation.rs
src/error.rs
src/governance.rs
src/hooks.rs
src/hooks/boundary_ledger.rs
src/hooks/claude_prompt.rs
src/hooks/claude_source.rs
src/hooks/claude_stop.rs
src/hooks/codex_prompt.rs
src/hooks/codex_stop.rs
src/hooks/turn_hook.rs
src/host.rs
src/installation/plan.rs
src/layout_recovery.rs
src/layout_recovery/command.rs
src/layout_recovery/journal.rs
src/layout_recovery/mutation.rs
src/layout_recovery/validation.rs
src/layout_recovery_context.rs
src/layout_recovery_publication.rs
src/layout_recovery_retention.rs
src/lib.rs
src/omp_boundary.rs
src/omp_installation.rs
src/pack_validation.rs
src/process.rs
src/recovery_transactions.rs
src/release_distribution.rs
src/semantic_sharing.rs
src/sharing_frontier.rs
src/sharing_hold_mapping.rs
src/sharing_page_export.rs
src/updating.rs
tests/obligations/python-baseline-v1.json
tests/obligations_ledger.rs
tests/t0_process.rs
tests/t1_recovery_transactions.rs
tests/t2_assembly.rs
tests/t2_boundaries.rs
tests/t2_cli.rs
tests/t2_delivery.rs
tests/t2_governance.rs
tests/t2_hooks.rs
tests/t2_installation.rs
tests/t2_layout_recovery.rs
tests/t2_omp_boundary.rs
tests/t2_omp_transport.rs
tests/t2_routes.rs
tests/t2_sharing.rs
tests/wiring-pending.txt
```

### Whole-suite diagnostic integration correction

The first checkpoint-2 whole-suite run completed in 600 seconds with only `t2_diagnostics` failing: two tests retained assertions that the memory-refresh provider was missing. Main approved updating `tests/t2_diagnostics.rs`, conditioned on reading the Python baseline first.

Baseline evidence (read only; no Python execution):

- `tests/contracts/test_runtime_diagnostics.py::call` defaults to expected exit **0**.
- `RuntimeDiagnosticsContract.test_pending_runtime_and_bank_transactions_are_observed_without_recovery` calls doctor with that default, asserts both runtime/bank transaction findings, `busy_or_interrupted`, and unchanged filesystem state.
- `diagnostics.py` lines 549–570 run `memory_refresh.local_prerequisites` first, add a **pass** for `bank.refresh_prerequisites` on success, then classify the bank marker as a **warning** and return without deep inventory.
- `diagnostics.execute` returns 0 when there are no failures; warnings alone do not change it.
- `RefreshPathPolicyIntegration.test_actual_protected_775_chain_current_and_doctor_preserve_feature_work` explicitly asserts the refresh prerequisite check is `pass`, `remote_check` is `not-exercised`, and `remote_freshness_verified` is false.

The updated native pending-transaction test therefore requires exit 0, warning-state runtime/bank transaction findings, no failures, passing refresh prerequisites, unchanged bytes, and no remote-freshness claim. The uninstalled-disposable-bank native smoke test still requires exit 1 for its independent runtime-artifact and unported recovery-frontier bank-inspection failures; it now explicitly requires passing local refresh prerequisites and preserves all its no-write/state assertions.

Additional approved modified file: `tests/t2_diagnostics.rs`. No diagnostic production behavior was weakened. After this test integration correction, fmt is clean, all-targets check has zero warnings, and `cargo test --offline --features dev-tools --test t2_diagnostics` passes **24 tests**, with 6 dependency ignores. The corrected whole-suite rerun is recorded below when complete.

### Final checkpoint-2 finish line

`cargo test --offline --no-fail-fast --features dev-tools` is **green** on macOS: **1,251 passed across 82 suites, 301 declared ignores**, outer wall time **575.31 seconds**. Retained full output: `artifact://14342`. Together with the final clean formatting check, zero-warning all-targets check, and three consecutive green lane/shared-fixture target runs recorded above, this meets the lane finish line. Temporary ledger input files were removed. No additional audits or checks were run after completion.
