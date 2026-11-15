# sharingholds lane report

## Scope and files

Worktree: /opt/agent-memory-worktrees/rust-v015-lane-sharingholds, detached at the assigned integration snapshot. No Git writes, Python invocations, new crates, subagents, or changes outside the assigned slice and coordinator-approved shared files.

Created:
- src/sharing_holds.rs
- tests/t2_sharing_holds.rs

Modified, with Main approval:
- src/lib.rs: alphabetical sharing_holds module registration.
- src/error.rs: append-only Kind::HoldFailure { code: String, repair: String } and crate-visible ContractError::hold_failure constructor at the end of the impl.
- tests/obligations/python-baseline-v1.json: six partial obligations and one ported obligation listed below, through am-dev obligations mark.

External handover artifact: /Users/robert.lee/.local/state/agent-memory/research/rust-cutover/sharingholds-report.md (this file). Temporary obligation-id files under /tmp were removed.

## Implementation and compatibility

Every baseline sharing_holds function and branch is implemented: recovery_needed, cause_id, locations, seal, registry_value (persisted and successor validation, structural-before-capacity checks and record/history/byte precedence), trusted_read, anchor_value, legacy_paths, import_legacy, read_store, filtered_records, load, prepare_directory, commit_store, bank_owner, record, and recover. The baseline HoldFailure is represented by ContractError with Kind::HoldFailure; callers match the kind for its exact code/repair while message()/Display retain the baseline reason.

Private state stays at writes/<sha256(bank path)>/semantic-holds/<project UUID>/{registry.json,recovery.json,journal.json,lock}, with the sibling <project UUID>.present.json anchor and retained per-cause <64-lowercase-hex>.json predecessor files. The codec is developer_identity::encoded: sorted keys, two-space indentation, Unicode preserved, finite values, trailing LF; all checksums and observed digests bind those exact bytes. Cause identity binds only sorted source keys and classification, not wording.

Uses the existing durable local_registry replacement protocol, private mkdir, native flock class and exact semantic-review-holds / semantic-holds operation labels. Judgment-derived UUIDv4 operation IDs preserve Python's version/variant bit rewriting; recovery uses one UUID for both locks. Commit order remains complete validation, journal write/fsync/rename, registry write, recovery-copy write, anchor verification/creation, exact readback, journal unlink and parent fsync, with all three baseline fault stages. Recovery does not overwrite a generation other than its observed or desired digest, retains unknown evidence, restores the exact backup after registry loss, imports only validated legacy rows, and requires a fresh source review afterward.

Scope identity is required before mkdir or stale-marker clearing; the same scope is checked again under actual bank ownership. Missing providers refuse rather than allowing recovery. records::reference is called directly. Reads use named MAX_STORE, MAX_JOURNAL, MAX_MARKER, MAX_LEGACY bounds and a MAX_RECORDS+64 namespace inventory; the baseline complete-private-input budget refusal is retained. Pinned no-follow reads recheck ownership, alias and mode policy on the actual read file. Written records are 0600; created state directories are 0700; existing permissions are not repaired.

## Public API and wiring

Public baseline functions are listed above. ReadOptions carries owner/allow_journal/allow_legacy; RecordOptions carries resolutions, owned_lock, verify callback and expected_inventory. Locations names the validated root/bank/directory/anchor. read_store_at, commit_store and recover_store operate on that located state; the latter is the shared durable recovery body, called by recover only after scope validation and both locks. Helpers provide explicit environment injection without process-global environment mutation.

Per Main's runtime-boundary ruling, these functions have an explicit runtime_root: &Path parameter: locations, locations_with_environment, read_store, load, record, record_with_environment, recover, recover_with_environment. The CLI must supply the installed executing runtime/source root; no production CARGO_MANIFEST_DIR, current_exe discovery or compile-host path is used. State under that root refuses with the baseline message. This module adds no CLI declaration; existing semantic orchestration will call the typed API when it lands.

Existing registration traits reused, with NO extensions:
- bank_writes::ScopeBinding::require_transaction(..., expected: Option<&Value>, owned_lock, owned_transaction=None, initialize=false), via scope_binding_provider(). Future implementer: scope_binding.
- bank_writes::TurnSources::canonical_writer_barrier, via turn_sources_provider(). Future implementer: turn_sources.

The leaves lane's sharing_hold_mapping.rs was read read-only. It expects HoldCauses::cause_id(&Value) -> Result<String>. Plain public cause_id is ready, and the zero-sized sharing_holds::HoldCauses adapter is defined. Once that module lands, add this one-line impl inside sharing_holds.rs:

`impl crate::sharing_hold_mapping::HoldCauses for HoldCauses { fn cause_id(&self, cause: &Value) -> Result<String> { cause_id(cause) } }`

Then CLI registration is sharing_hold_mapping::register_hold_causes(Box::new(sharing_holds::HoldCauses))?. No replacement registration convention was introduced.

## Tests and obligation ledger

The t2_sharing_holds target has 16 real tests, no ignored tests, no fake scope/turn provider. Main approved the same narrow FixtureLabels input constraint as tests/t1_layout_recovery_state.rs: repository-config has_reasons asserts the sole fixed fixture label is exactly memory-bank and returns false; scan(body) always refuses. This does not implement or claim security scanning.

Tests cover literal codec/checksum bytes; cause-id input bytes and wording independence; all three exact successor-capacity boundaries and repeated refusals preserving files/modes; structural and combined-capacity precedence; persisted corruption versus capacity; exact resolution predecessors and filtered inventory reconstruction; actual record writes under a native bank lock, identical replay, first-detection retention, repeated resolution renewal and expected-inventory drift; registry-loss recovery; legacy import and retained original bytes; bounded legacy inventory; all journal replay generations including later-state refusal; unknown journals/changed content/changed anchors/incomplete backups; written modes; owner marker bytes, native flock contention, busy reads and cleanup; symlinks, writable records and explicit read bounds; direct records reference validation; explicit runtime exclusion; and missing registration refusal without writes.

Fully ported:
- tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_structure_precedes_capacity_and_combined_dimension_precedence

Partial (module-level behavior ported; remaining public inspect/review/check receipts, source-quality judgments, canonical Git state assertions and proof validation require semantic_sharing and related later sharing modules):
- tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_record_boundary_excess_and_retry
- tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_history_boundary_excess_and_retry
- tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_sealed_byte_boundary_excess_and_retry
- tests/contracts/test_known_hold_capacity.py::KnownHoldCapacityContract.test_persisted_corruption_and_interruption_remain_noncapacity
- tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_renewed_clarification_proof_replays_without_losing_prior_resolution
- tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_known_dispute_survives_rewording_changed_framing_and_registry_recovery

Deferred, ledger left unported; no placeholder/irrelevant ignored bodies were retained (Main explicitly approved this disposition):
- tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_indexed_review_requires_every_pair_and_preserves_conflict — needs semantic_sharing + sharing_hold_mapping.
- tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_indexed_review_refuses_changed_source_or_authority — needs semantic_sharing + sharing_hold_mapping.
- tests/contracts/test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_wrong_project_recovery_preserves_private_state_and_stale_bank_marker — needs actual scope_binding + turn_sources registration and its project/scope fixture.

The later publication_hold_recovery / checked_page_export integration contracts are not claimed by this slice. No baseline sharing_holds implementation branch is deferred; only the named cross-module tests/wiring depend on later modules.

Ledger commands executed successfully:
- cargo run --offline --features dev-tools --bin am-dev -- obligations mark --status partial --ids-file /tmp/am-sharingholds-partial-20261113.ids
- cargo run --offline --features dev-tools --bin am-dev -- obligations mark --status ported --ids-file /tmp/am-sharingholds-ported-20261113.ids

## Declared deltas and foundation changes

- Runtime/source exclusion is an explicit typed caller input instead of Python's __file__ discovery, as directed by Main. Paths and refusal text do not change.
- Non-UTF-8 filesystem names/JSON and the foundation codec's BOM, lone-surrogate and excessive-nesting cases receive typed refusals rather than interpreter-dependent behavior, per lane brief. A non-UTF-8 retained namespace entry refuses rather than being silently ignored.
- Read file identity/trust is enforced on the pinned file rather than allowing a path re-resolution race; no weaker alias/symlink fallback.
- Malformed direct typed arguments that Python would raise on are typed refusals, never input-triggered panics. Durable accepted formats, bounds and intended refusals are unchanged.
- Foundation changes: only the approved typed HoldFailure variant/constructor in error.rs. No filesystem, process, host or codec primitive was changed.

## Verification

Platform observed: aarch64-apple-darwin; Darwin 27.3.0, kernel xnu-13432.60.57~99/RELEASE_ARM64_T6050; rustc 1.99.1 (58cf8f9c35b355199727627d07ae34a4e2846d67, 2026-10-13), LLVM 23.1.1. No Linux/musl or x86_64 execution/cross-build was run by this lane.

Commands: rustc -vV; uname -a; printf '{}\n' | shasum -a 256 (literal baseline empty-object digest); formatter output computed with rustfmt --edition 2024 --emit stdout on stdin and applied through hashline edits (not direct formatter writes); then the required cargo checks below. No unwrap()/expect()/range slicing matches were found by the mandatory source grep before the final rerun. The two direct UUID digit positions are guarded by an immediately established ASCII-hex length invariant and explanatory comments.

Earlier check iterations exposed and fixed two empty-object type inferences, test digest LowerHex formatting, order-sensitive test Value equality, the missing constrained fixture label scanner, and strict resolution of not-yet-created state. A subsequent whole suite passed, then the bounded-read refusal mapping was corrected to its exact baseline text and all required final checks were rerun on the final candidate.

Final candidate checks all passed (single final command chain, exit 0, 206.29 seconds):
- cargo fmt --check — passed.
- cargo check --offline --all-targets --features dev-tools — passed, no warnings.
- cargo test --offline --test t2_sharing_holds — passed three consecutive times: 16 passed, 0 failed, 0 ignored; 4.05s / 4.07s / 4.06s.
- cargo test --offline --no-fail-fast — whole suite passed; the only two ignored tests are the pre-existing bank_transactions tests requiring security/recovery_frontier. obligations_ledger passed, including live_ledger_is_consistent. The holds target also passed in the whole-suite run (16/16).
- grep -n 'unwrap()\|expect(\|\[.*\.\.' src/sharing_holds.rs — no matches; the command chain explicitly normalizes grep's no-match exit to success.

No check was rerun merely to confirm a user-reported observation. No source changes followed the final successful verification.

Accountability: awa status --json observed managed=true for robertelee78/agent-memory (awa 0.10.15). The read-only managed Rust-issue search did not identify an unambiguous sharing-holds story; no phase/health was changed or reconciliation receipt claimed by this lane. The coordinator owns integration-wide tracking. vox agent status --harness omp observed node omp-work Ready and this worktree's bound room kh57aq2zwdss; no missing setup was reported.
