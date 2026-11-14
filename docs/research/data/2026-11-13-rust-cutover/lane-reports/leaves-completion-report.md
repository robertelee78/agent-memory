# Leaves completion report

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-leaves`.
Final prerequisite snapshot: committed integration `2a12544a8e82e1e922f814f8c27540df471cffa0`.
No Python execution, network access, subagents or Git writes. Main commits the result.

## Files to integrate

Implementation changes: `src/client_hooks.rs`, `src/hook_failure.rs`, `src/managed_source.rs`, `src/turn_check.rs`, `src/sharing_hold_mapping.rs`, `src/package.rs`, `tests/t2_leaves.rs`, six alphabetical module declarations in `src/lib.rs`, `Cargo.toml`, `Cargo.lock`.
Approved foundation changes: `src/fs/descriptor.rs` (`Descriptor::read_chunk` only, on top of the committed descriptor implementation), `tests/t0_fs.rs` (one appended test only).
This report is the only new documentation file. The earlier `leaves-report.md` is not overwritten.

All non-owned files under `src/`, `tests/`, and `skills/agent-memory/schemas/`, plus the manifest/lockfile/bootstrap, were refreshed. The final refresh used `git show` from the committed snapshot, not the integration working files. Retain the integration versions of these prerequisites; they are not independent leaves changes. In particular, the earlier local `entries_bounded` edit is gone: committed `entries`, `entries_bounded`, directory creation/removal and their safety checks are retained.

Dependency approved by Main: `sha1 = "=0.11.0"`. Cargo resolved exactly one additional package, `sha1 0.11.0`; it uses the already resolved `cfg-if`, `cpufeatures`, and `digest` dependencies. The integration's `flate2` dependency and binary/feature declarations are retained unchanged.

## Defects in the previous handover and fixes

1. **Stale prerequisite tree and local descriptor fork.** Refreshed from the committed integration snapshot and retained its bounded directory implementation, rather than carrying the previous worker's duplicate foundation change.
2. **Spec carried an effectively unbounded managed-source deadline.** Both the process specification and its spawn-relative supervision now explicitly use 30 seconds. A real sleeping Git replacement proves that the direct child is killed and reaped; hook repository discovery retains its baseline five-second deadline.
3. **Object and blob hashes were delegated to an extra `git hash-object` child.** SHA-1/SHA-256 hashes now run in-process over the exact Git object header and bytes, selecting the algorithm from the recorded object-ID width. This removes an invented subprocess, Git-config dependence of hashing, and stdin copying per blob. Both SHA-1 and actual SHA-256 repositories are covered.
4. **Blob reads allocated the complete file.** Replaced with a 65,536-byte stack buffer and the approved descriptor primitive. Reads request at most the remaining named limit plus one byte; growth is refused, and descriptor/name identity and size are rechecked. No complete-file buffer is allocated.
5. **Git stdout overflow lost the baseline refusal.** Stdout capture-limit errors now say `Legacy managed source: Git object exceeded its bound while reading`, rather than the generic Git-observation error.
6. **Bounded Git stderr was captured, contrary to the baseline's DEVNULL.** It now goes to a held `/dev/null` descriptor. The malformed-size fixture emits 100,000 stderr bytes and still reaches the correct size-validation refusal; discarded stderr cannot create a spurious capture-limit refusal.
7. **Git size parsing accepted a leading plus sign.** Size reports must be nonempty ASCII digits after byte whitespace trimming, before conversion and bound checking.
8. **Tree-parser acceptance differed for an empty tree-entry name.** Restored the baseline delimiter predicate. Structural bounds and all later ownership checks still apply.
9. **Duplicated hex encoding and structural JSON equality implementations.** Object IDs use the existing `validation_memo::hex`. Structural Python JSON equality is shared with the client-hook membership implementation; serialized hook ownership equality remains deliberately distinct.
10. **Untrusted indexing/unwrap/expect sites throughout the ports.** Replaced them with checked access and typed refusals. The six-module required grep has zero hits. Hook removal's vector position is established by `locate` on the same unchanged array and is documented immediately before removal.
11. **Client command whitespace did not match Python `str.strip`.** U+001C through U+001F now count as whitespace in addition to Unicode whitespace. The byte limit remains 4,096.
12. **Compact batch context comparison did not reproduce Python JSON numeric equality.** Session/prompt comparisons now use the shared structural comparison, including boolean/integer/float equality where Python uses it. Observation-ID shape validation precedes duplicate-ID classification.
13. **Completion providers could express only contract errors.** Added `CompletionResult<T>` and `CompletionError::{Contract(ContractError), Unexpected}`. Contract reasons and bank-capacity details survive; unexpected unavailable-state failures receive exactly `Original source or private assessment state is unavailable.` without exception content. Provider traits carry the distinction as well.
14. **Diagnostic encoding errors silently became an empty string.** `failure_notice` now returns a typed result and `emit` propagates it. Quiet success, actionable failure text, notice suppression and exit-code rules remain baseline-shaped.
15. **Absent hook slots were subjected to state validation too early.** A missing directory/slot now returns absence before private-state validation, as `read_one` does in Python. Existing slots remain pinned, bounded and verified.
16. **Timestamp parsing was a large speculative partial reimplementation of CPython ISO parsing.** Removed it. Validation now accepts exactly the writer-reachable UTC forms described below, using the existing calendar validator. Unsupported forms are an explicit declared delta, not an unverified compatibility claim.
17. **Mapping truthiness treated numeric zero as true.** Integer and float zero now follow Python truthiness. Mapping equality also uses the shared structural Python comparison.
18. **`current_rows` copied complete source rows and bodies.** It now returns a borrowed map; indexing and resolution validation do not clone those bodies.
19. **`package.rs` consisted only of invented unused declaration constants and omitted engine generation.** Removed those constants. Added artifact-bound `engine_generation`; no interpreter probing, bytecode inspection, source-tree generation or Python-era identity is reproduced.
20. **Eight tests did not exercise the filesystem/process contracts.** The target now has 35 tests, including real private repositories, linked worktrees, modes, unsafe links, corruption, exact metadata bytes, staged/index-hidden changes, forged object identities, ambient Git isolation, growth and actual owned-process overflow/deadline cleanup. No missing domain is replaced by a fake implementation.
21. **Installation needed shared helpers rather than another hardened-Git or shell-quoting implementation.** Exported the requested existing `git` and `shell_quote` helpers. Literal shell-quote vectors cover the complete safe set, empty input, apostrophes, quotes, spaces, Unicode, newlines and metacharacters.

## Function-by-function coverage and public API

- `client_hooks`: baseline `parse`, `encode`, `entry`, `equal`, `locate`, `merge`, `remove` remain available with their existing Rust signatures. Preserve duplicate-key refusals, passthrough JSON encoding/order, structural collision semantics versus serialized ownership comparison, and original empty-hook shape restoration. `shell_quote(&str) -> String` is public and byte-exact for the requested CPython `shlex.quote` rules. `python_era_entries` and `rust_era_command` remain installation/adoption helpers; they do not execute Python.
- `hook_failure`: `paths`, `validate`, `read_raw`, `read_one`, `read`, `write`, `record`, `clear` retain their public signatures and `NoRepository` classification. Tests cover exact sorted ASCII JSON bytes without a trailing newline, 0600 records/locks, 0700 diagnostic directories, readonly absence, independent adapters/worktrees, bounded oversized-prefix cleanup, corruption replacement, and preservation/refusal of unsafe slots.
- `managed_source`: baseline requirement helpers, Git observation, object verification, tree payload traversal, blob identity, index verification, actual payload verification and receipt/channel verification are implemented. Existing `payload`, `verify_index`, `verify_payload`, `verify`, result structures and installation registry are retained. Added public `git(root: &Path, args: &[&str], input: Option<&[u8]>, maximum: usize) -> managed_source::Result<Vec<u8>>` (successful stdout only), and `blob_identity(oid_bytes: usize, parent: &Descriptor, name: &OsStr, observed: Identity) -> managed_source::Result<String>`; valid widths are 20 and 32. `Installation::source_info` must actually verify payload ownership before claiming cleanliness.
- `turn_check`: `requests`, `notice_required` (including the baseline private notice helper through its locked store), `check`, `combined`, `exit_code`, `failure_notice`, `emit` are implemented. `check` now takes a completion callback returning `CompletionResult<Value>`. `failure_notice(&Value) -> error::Result<String>` is the other signature correction. `emit` still returns status/stdout/stderr. The source-assessment failure structure retains its original field order and capacity detail extension.
- `sharing_hold_mapping`: `current_rows`, `source_index`, `successors`, `describe`, `validate_resolution`, `qualified_current`, `cached` are implemented. All resolution bases retain their baseline refusal text and output fields. `current_rows(&Rows) -> Result<BTreeMap<&str, &Value>>` is the borrowing correction; the other public signatures remain stable. Source IDs/corrections are sorted at the same externally visible successor/description boundaries.
- `package::engine_generation(pack: &Path, binary: &Path) -> pack_validation::Result<String>` freshly obtains the complete artifact digest through `artifact::inspect_runtime`. Errors disable optional cache reuse; they never invent a generation. The only baseline consumer of `_ENGINE_SOURCE_ROOT`, `_INITIAL_ENGINE_SOURCES`, `_engine_sources` is `recall_catalog.generation_details`, which also computes loaded bytecode/interpreter data. Its Rust consumer must use the artifact digest and treat every Python-era recall generation as a cache miss. `migration-catalog-v2` is taken from the refreshed committed schema and existing `migration_contract`; no v1 Python module loader is introduced.

### Required provider wiring (not fake implementations)

- `managed_source::Installation`: implement/register in `installation`.
- `turn_check::ScopeBinding`: `scope_binding`; `MemoryRefresh`: `memory_refresh`; `TurnSources` and locked `NoticeStore`: `turn_sources`. Store replacement/removal must hold the source barrier and namespace lock, perform enrollment rechecks, and durably sync actual state.
- `sharing_hold_mapping::HoldCauses`: `sharing_holds`; `SemanticSharing`: `semantic_sharing`; `SecurityScan`: `security`.

A missing registration is a named typed integration refusal. No baseline function is silently replaced by success. Integration-dependent execution and test gaps are enumerated below.

### CLI declarations

None of the six Python baseline modules has an argparse CLI. `turn_check.emit` is a typed result/output operation for its CLI/harness callers; there are no flags/defaults/choices to port from these modules themselves.

## Timestamp call-site enumeration and declared deltas

There is exactly one `datetime.fromisoformat` call site in these six baselines: `hook_failure.validate`, reached by reading or writing hook-failure records. The only production writer of its `timestamp` is `hook_failure.record`: `datetime.now(datetime.timezone.utc).isoformat()`.

Writer-reachable forms accepted by Rust:

- `YYYY-MM-DDTHH:MM:SS+00:00` when microseconds are zero.
- `YYYY-MM-DDTHH:MM:SS.ffffff+00:00` when microseconds are nonzero, with exactly six decimal digits.
- Gregorian years 0001–9999, real calendar dates, hours 00–23, minutes/seconds 00–59; no leap seconds. The existing 40-character metadata bound is retained.

Declared timestamp delta: refuse every other CPython `fromisoformat` form, including `Z`, nonzero or negative-zero offsets, offsets containing seconds/fractions, compact dates/times, ISO week dates, arbitrary/space/Unicode separators, reduced time precision, comma fractions, non-six-digit fractions, and an explicitly zero six-digit fraction. None is emitted by this field's writer. The tests include both canonical forms, year/calendar boundaries and negative examples. No interpreter vector request was necessary to establish the writer grammar; no claim of implementing the whole CPython parser is made.

Other shared cutover deltas: strict UTF-8/no BOM or UTF-16/32 JSON; lone-surrogate refusal; codec nesting bound 256; deterministic sorted inventory traversal rather than filesystem enumeration accidents; explicit finite bounds on formerly unbounded Git output. Malformed untyped mapping structures return typed refusals instead of incidental Python exceptions. Newly missing private-state ancestors are created with T0's deterministic private 0700 policy (rather than Python's umask-derived intermediate-directory modes); existing ancestors/records are never silently chmod-repaired.

## Obligations exercised and remaining dependency gaps

Each ported or partially ported contract has its exact obligation comment above the Rust test. Standalone supplemental tests additionally cover the public APIs and precise bytes.

### Fully exercised standalone leaf obligations

- All four `test_client_hooks.py::ClientHookContract` methods.
- `test_hook_failure.py::HookFailureContract.test_independent_adapters_worktrees_private_bounds_and_readonly_absence`.
- `test_unsafe_or_oversized_metadata_is_preserved_and_refused`.
- `test_malformed_fields_are_refused_and_missing_clear_writes_nothing`.
- `test_non_git_file_only_diagnostics_are_not_hook_failures`.
- `test_linked_worktree_rejects_state_in_main_checkout_memory`.
- `test_one_corrupt_adapter_does_not_hide_other_failure`.
- All five `test_managed_source_bounds.py::ManagedSourceBoundsContract` methods. The growth test uses a real 64-MiB file and concurrent growth rather than Python's mocked `os.read`/temporary 16-byte limit; it proves refusal/preservation, while which identity/bound check observes the race is scheduling-dependent. Link/FIFO replacements use actual objects and cannot block the verifier. Overflow and deadline tests inspect the actual child PID after supervised collection.

### Partially exercised; exact remaining branches

- `test_managed_source_ownership.py::ManagedSourceOwnershipContract.test_clean_index_refresh_previews_updates_and_repeats_preserving_old_source_and_bank`: real Git index refresh changes raw index bytes without invalidating payload ownership. Preview/update/repeat/backups/bank preservation await `installation`, `updating` and their lifecycle integration.
- `test_hidden_tracked_changes_extra_ignored_files_modes_and_channel_are_preserved_and_refused`: covered hidden-by-index, skip-worktree, staged-only, staged-deletion, ignored-extra, empty-directory, executable intent, forged commit/tree, alternate object stores, changed origin and detached HEAD; file/index bytes are preserved. `package-mode`, `root-mode` and `wrapper` ownership/fingerprint cases, and the full updater invocation, await `installation`/`updating`.
- `test_local_ownership_does_not_execute_fsmonitor_or_filters_or_follow_ambient_git_directory`: actual raw ownership verification ignores ambient Git paths/config and does not execute configured traps or rewrite the index. Installed `am version`, `doctor`, and update-preview assertions await `installation`, `diagnostics` and `updating`.
- `test_known_holds_v2.py::KnownHoldsVersionTwoContract.test_known_dispute_survives_rewording_changed_framing_and_registry_recovery`: standalone mapping covers stable record IDs across rewording/moves, exact-version corrections, tombstone witness IDs, logical fallback references and missing endpoints. Actual detection, persisted registry loss/recovery/import and publication remain dependent on `sharing_holds`, `semantic_sharing`, `security` and `memory_refresh`.

### Remaining hook-failure tests, each awaiting its real harness/domain

- `test_hook_failure_codes_survive_without_source_content`: Codex prompt/stop adapters, `turn_sources`, `scope_binding`.
- `test_unresolved_identity_retains_fixed_metadata_without_bank_change`: Codex adapters, `scope_binding`, `turn_sources` and admission fixture integration.
- `test_source_compatibility_fault_is_not_misclassified_as_identity`: Codex stop adapter and `turn_sources` compatibility/enrollment wiring (layout recovery state itself is in the prerequisite snapshot).
- `test_stop_scope_loss_reports_identity_code_without_bank_change`: Codex stop adapter, `turn_sources`, `scope_binding`.
- `test_unexpected_exception_is_fixed_and_does_not_expose_exception_text`: actual Codex adapter error classification around repository/bank integration.
- `test_bank_absence_clears_only_its_own_adapter`: actual Codex prompt adapter/bank-absence route.

### Remaining managed-source ownership tests

- `test_actual_v002_runtime_failure_is_repaired_only_by_outside_helper`: genuine legacy-runtime qualification outside this Rust-only lane; `installation`/`updating`. No fixture interpreter is run here.
- `test_interrupted_switch_restores_exact_drifted_source_and_retries`: `installation`, `updating`, transaction fault/recovery integration.
- `test_doctor_uses_the_same_legacy_ownership_check`: `diagnostics`/doctor and installed-runtime `installation` wiring.

### Remaining known-hold tests

Each needs actual `sharing_holds`, `semantic_sharing`, `security` and source-bound public review/CLI integration; none can truthfully run with just this mapping leaf:

- `KnownHoldsVersionTwoContract.test_indexed_review_requires_every_pair_and_preserves_conflict`.
- `test_indexed_review_refuses_changed_source_or_authority`.
- `test_renewed_clarification_proof_replays_without_losing_prior_resolution`.
- The persisted recovery/import part of `test_known_dispute_survives_rewording_changed_framing_and_registry_recovery` (also `memory_refresh`).
- `test_wrong_project_recovery_preserves_private_state_and_stale_bank_marker`.
- `KnownHoldCapacityContract.test_record_boundary_excess_and_retry`.
- `test_history_boundary_excess_and_retry`.
- `test_sealed_byte_boundary_excess_and_retry`.
- `test_structure_precedes_capacity_and_combined_dimension_precedence`.
- `test_persisted_corruption_and_interruption_remain_noncapacity`.

`validate_resolution`/`qualified_current`/non-null `cached` execution specifically waits for the real semantic/security providers; `describe` waits for the real cause-ID provider. No fake evidence-safety provider or synthetic success registration was added to claim these obligations.

### Remaining compact-turn integration tests

All four `CompactTurnCheckContract` methods in `test_compact_turn_check.py` await `turn_sources`, `scope_binding`, `memory_refresh`, their Codex/OMP adapters and CLI integration:
`test_actual_cli_one_call_quiet_and_exact_legacy_receipt_scope`, `test_actual_assessment_refresh_failure_dedup_and_recovery`, `test_batch_distinct_observations_and_partial_failure`, `test_native_combined_refresh_uses_protected_writable_state_without_review_echo`.
Standalone tests cover requests/refusal order, JSON numeric equality, error distinction/redaction, combined observations, quiet notice text and exit-code identity rules; they do not claim source completion or durable notice-store integration.

## Foundation change

`Descriptor::read_chunk(&mut self, buffer: &mut [u8]) -> io::Result<usize>` performs one allocation-free `read(2)`, retries only EINTR, and returns zero for EOF/empty buffers. It preserves the descriptor's native FIFO blocking/nonblocking semantics; callers validate file type before use. The test covers multi-chunk offsets, empty input, directory refusal, and nonblocking FIFO EOF/WouldBlock/data behavior. The managed-source caller pins and checks identity, counts bytes against `MAX_FILE`, and rechecks identity after hashing. Existing committed descriptor methods are not replaced.

## Verification actually run

- `cargo fmt --all` — successful after edits.
- `cargo fmt --check` — final clean.
- `cargo check --offline --all-targets` — final successful, no warnings.
- `cargo test --offline --test t2_leaves` — final three consecutive runs: **35 passed / 0 failed** each (30.07 s, 30.06 s, 30.06 s). Earlier intermediate runs passed 32/32 once, 33/33 three times and 35/35 three times before the final stderr correction.
- `cargo test --offline --test t0_fs descriptor_chunks_preserve_offsets_and_native_fifo_semantics` — **1 passed / 0 failed**, final run; also passed at earlier checkpoints.
- Exact required grep: `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/{client_hooks,hook_failure,managed_source,turn_check,sharing_hold_mapping,package}.rs` — **no matches**. No production hit needs a waiver.

One intermediate all-target check caught a test calling crate-private `lifecycle::object`; the test now directly constructs `Value::Object`. The final all-target check and all final tests above include that correction. No other build/test failure is hidden, and no full-suite or Linux result is claimed.
