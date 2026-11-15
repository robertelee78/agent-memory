# BankFix review remediation

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-bankfix`, based on detached integration commit `2a12544`. No Git writes, Python execution, or subagents. Review: `docs/research/data/2026-11-13-rust-cutover/lane-reports/bank-trio-review.md` (read completely).

## Files changed

- `src/bank_contract.rs`
- `src/bank_layout.rs`
- `src/bank_writes.rs`
- `tests/t1_bank.rs`
- `src/error.rs` (Main-authorized shared extension)
- `src/records.rs` (Main-authorized missing-parser refusal only)
- `src/fs/descriptor.rs` (Main-authorized foundation change)
- `tests/t0_fs.rs` (Main-authorized foundation tests)
- `tests/obligations/python-baseline-v1.json` (Main-authorized partial marks)
- This external report: `/Users/robert.lee/.local/state/agent-memory/research/rust-cutover/bankfix-report.md`.

## Review disposition and coverage

| Review item | Change | Test(s) |
| --- | --- | --- |
| P2-1: nonexistent fault rendezvous | Base already resolved non-strictly; added real subprocess pause regression. The directory and nested parent do not exist beforehand; `.ready` must be empty and visible while the child remains paused, then `.release` permits completion. Test roots explicitly use canonical `/tmp`, matching the disposable-world contract on macOS. | `pause_fault_creates_missing_rendezvous_and_writes_ready`, `isolated_bank_fault_child` |
| P2-2: non-UTF-8 sibling | Base already skips unrelated non-UTF-8 sibling names. Added Linux-only regression with raw `caf\xe9.txt`, checking `bank_root` accepts the bank and preserves the sibling. macOS refuses this filename, so the regression is cfg-gated and was not executed on this host. | `non_utf8_repository_sibling_does_not_alias_the_bank` |
| P3-1: table refusal order and lone pipe | Validate all rows before duplicate detection; `|` has an empty interior and therefore one cell, yielding the baseline five-column refusal. | `route_table_validates_later_rows_before_duplicate_paths`; strengthened exact strings in `router_bounds_duplicate_routes_and_orphan_files` |
| P3-2: line splitting and exact rendering | One allocation-free Python `splitlines(keepends)` iterator recognizes LF, CR, CRLF, VT, FF, FS/GS/RS, NEL and Unicode line/paragraph separators; CRLF is coalesced and terminal breaks do not create a final empty line. Used for table parsing, omission parsing, insertion and omission filtering. LF-only frontmatter substitution stays LF-only, matching the baseline multiline regex. | `add_context_routes_has_byte_exact_golden_with_python_line_boundaries` (literal golden, including CRLF omission lists and VT+LF row boundary); `add_context_routes_preserves_lf_only_frontmatter_substitution` |
| P3-3: replacement error precedence and creation cleanup | Missing temporary is already cleaned; cleanup is attempted before propagating the original replacement result, whose error wins. `create_exclusive` now obtains initial status and removes its own created entry if mode setting fails (only if identity still matches); initial-stat failure removes the created name as the baseline does. The bank's subsequent initial-stat failure also explicitly unlinks its just-created name before returning the original error. | Active foundation `post_create_mode_failure_removes_only_the_created_file`, `descriptor_mode_changes_the_held_inode_and_rejects_nonpermission_bits`; deferred protocol `replacement_error_wins_when_the_temporary_was_removed_and_cleanup_leaves_no_files` |
| P3-4: empty HOME | Construct the default using `expanduser_with_home("~/.local/state", home)`, then append `agent-memory`; `HOME=""` resolves to `/.local/state/agent-memory`. | `state_root_resolves_unset_empty_and_explicit_home_without_creating_it` (unset, empty, explicit missing home; no directories created) |
| P3-5a: hard-linked receipt replay | Restore baseline acceptance using the foundation bounded no-follow reader with `require_single_link=false`. No weakened symlink policy or unbounded read. | Active `canonical_receipt_path_record_load_roundtrip_accepts_hardlinks`; deferred full replay `already_present_then_receipted_replay_preserves_bytes_and_hardlinked_receipt` |
| P3-5b: missing tombstone parser masked as bad user data | Added typed `Kind::MissingProvider`; missing parser and bank-provider refusals retain their exact text with this kind. Optional-record error masking passes this kind through, including nested indices, while content failures remain redacted. | `missing_tombstone_registration_is_not_user_file_corruption_even_when_nested` (direct and nested); `real_observation_scans_sources_and_write_protocol_refuses_missing_providers_without_writes` |
| P3-5c: unlocked reader detects runtime lock churn | Kept the fail-safe directory/full-identity checks as a declared delta, with precise refusal: `Bank paths changed during inspection, including runtime lock state; preserve the existing layout and retry when writers are idle`. | Existing stable runtime traversal coverage: `physical_inventory_checks_runtime_descendants_without_following_links`. No timing-dependent race test claims deterministic coverage of the retained full-signature refusal. |

## Other named review test gaps

- Exact refusals now replace broad `is_err()` assertions in all five named weak tests: `byte_zero_schema_and_restricted_metadata_errors`, `missing_empty_and_omitted_existing_targets_fail`, `router_bounds_duplicate_routes_and_orphan_files`, `write_proposals_refuse_missing_scope_provider_and_invalid_paths`, `index_byte_row_and_unicode_framing_limits_are_enforced_before_a_valid_result`.
- `memo_keeps_companion_records_and_rechecks_router` changes an optional non-router record body while preserving the router and asserts a fresh redacted refusal; it still checks cached companions and changed-router validation.
- The committed literal minimal-router golden is shared with the receipt expectation, not derived from the renderer. The deferred full receipt golden manually spells baseline sorted-indent2 UTF-8 JSON and hashes literal baseline fixture bytes rather than reusing the receipt encoder or scanned hashes for its expectations.
- The real `bank_observation::BankObservation` provider is registered once in tests. Runnable real-file tests cover source/hash scans, exact missing-provider refusal before mutation, canonical UUID receipt path -> private `record` -> bounded `load_receipt`, exact receipt bytes, mode 0600, hard links and preserving unexpected bytes.

## Explicitly deferred protocol tests / obligations

No fake providers were added. Four concrete real-file tests remain ignored with named missing registrations:

1. `write_preview_apply_preserves_mode_readback_and_exact_receipt_bytes`: requires recovery_frontier, scope_binding, security, turn_sources. Exercises preview read-only behavior, low-level apply with checked real lock-owner bytes, target mode 0640, exact complete receipt bytes and mode 0600, readback, unchanged router and no temporary/lock leftovers.
2. `already_present_then_receipted_replay_preserves_bytes_and_hardlinked_receipt`: requires scope_binding, recovery_frontier, security, turn_sources. Exercises an already-present result without target replacement, already-receipted replay including a hard-linked receipt, later external content, `still_current`, receipt-byte preservation and no temporary leftovers.
3. `unexpected_receipt_is_preserved_during_apply`: same registrations. Preserves unexpected receipt and target bytes, asserts the exact operation-ID reuse refusal.
4. `replacement_error_wins_when_the_temporary_was_removed_and_cleanup_leaves_no_files`: same registrations. Real pause subprocess, external target edit, with and without external deletion of the created temporary, exact original error, no receipt/lock/temp leftovers.

The existing tree also refuses repository-root resolution before observation with `Repository boundary security scan provider is not registered`; the runnable test asserts this exact earliest refusal. It does not misrepresent that as a recovery-frontier refusal. Checkpoint-specific execution continues to require the unported checkpoint provider; this lane adds no checkpoint shim and does not claim checkpoint protocol coverage.

Marked **partial**, not fully ported, these exact obligation IDs:

- `tests/contracts/test_bank_writes.py::BankWritesContract.test_lock_precedes_inventory_and_second_writer_and_reader_stop` (fault rendezvous regression is active; full writer/reader ordering still deferred).
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_preview_and_inspect_are_read_only_and_apply_replaces_one_file_with_readback`
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_interrupted_after_replace_is_distinct_from_a_failed_write_and_replay_converges`
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_lost_response_after_receipt_replays_without_duplicate_or_replacement`
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_invalid_scope_symlinks_and_operation_id_reuse_refuse`
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_external_target_edit_immediately_before_replacement_is_not_overwritten`
- `tests/contracts/test_bank_writes.py::BankWritesContract.test_failure_before_replace_leaves_the_original_and_no_verified_receipt`

The four ignored tests cover internal protocol branches, not every CLI/identity setup, forced process-exit/recovery, inspect, invalid-scope and competing-writer subcase of those broad baseline obligations. Those remaining subcases are not claimed ported. Existing router/layout obligation statuses are unchanged.

## Public API / registrations / foundation

- Append-only public `Kind::MissingProvider`; crate-private `ContractError::missing_provider(message)` preserves user-facing text.
- Append-only public `Descriptor::set_mode(&self, mode: u32) -> io::Result<()>`: held-descriptor `fchmod`, rejects bits above `0o7777` with InvalidInput. Coordinate merge with the installation lane's same-signature method: retain one definition.
- `Descriptor::create_exclusive` now cleans up initial-stat and post-create mode failures, preserving the original error. Its guarded cleanup path is deterministically exercised by invalid permission bits `0o10000` after open+fstat; a real kernel `fstat` failure on a live owned descriptor is not forced by this test.
- No trait items, registration signatures, or provider accessors were reordered, renamed or reformatted; missing-provider expressions alone changed as requested. No new registration traits, CLI declarations, crates or dependencies.
- No production Python changes.

## Declared deltas kept

1. A bank directory (including runtime lock entries) changing during an unlocked physical walk can refuse instead of returning the baseline's potentially stale listing. The precise refusal above identifies runtime lock state and recommends retrying when writers are idle. No write or repair is attempted.
2. An absent required Rust registration is an infrastructure refusal (`Kind::MissingProvider`), not an invalid optional-record refusal. This is necessarily Rust-specific; user-file corruption remains redacted.

Hard-linked receipt refusal was removed, not retained as a delta. Other pre-existing lane safety bounds and declared deltas are unchanged.

## Verification

Host only: `aarch64-apple-darwin`, Darwin 27.3.0 / kernel `xnu-13432.60.57~99/RELEASE_ARM64_T6050`; rustc 1.99.1 (`58cf8f9c3`, 2026-10-13), LLVM 23.1.1. Linux and other target triples were not executed.

Commands run:

- `uname -a`; `rustc -vV` — platform above.
- `cargo fmt --check` — initially reported formatting differences; applied format-only hashline edits. Final invocation passed.
- `cargo check --offline --all-targets --features dev-tools` — initial test compile found two test-only type errors; fixed. Final invocation passed without warnings.
- `cargo test --offline --test t1_bank` — initial fixture run found the macOS temporary-root mismatch and the earlier repository-security registration refusal; fixed the fixture and exact expected message. Final target was green **three consecutive times**, each **39 passed, 4 ignored** (Linux-only sibling test cfg-excluded).
- `cargo test --offline --test t0_fs` — **42 passed**.
- `cargo run --offline --features dev-tools --bin am-dev -- obligations mark --status partial --ids-file /Users/robert.lee/.local/state/agent-memory/research/rust-cutover/bankfix-obligation-ids.txt` — passed; temporary IDs input removed after use.
- `cargo test --offline --no-fail-fast` — **passed** across all test targets and doc-tests; **6 ignored** total (4 bank, 2 transaction prerequisites). Full command output retained as `artifact://11203`; all reported test results are `ok`, no failed target.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/bank_contract.rs src/bank_layout.rs src/bank_writes.rs` — 11 matches: contract lines 24, 30, 39, 41, 252, 1059; layout 24, 503; writes 246, 1079, 1082. They are existing bundled-schema extraction, internally constructed output-object/digest invariants, plus the `vec![".."; ...]` false positive. No new unchecked input/registry access was introduced.

Coordination: initial AWA status was managed, but no unique bankfix story was available (`awa ready --json` could not access projects 72/73); no tracker writes occurred. Main explicitly owns tracker reconciliation and instructed this lane not to use further tracker tools. Vox initial status reported omp-work Ready in this worktree's bound room. No Git writes occurred.
