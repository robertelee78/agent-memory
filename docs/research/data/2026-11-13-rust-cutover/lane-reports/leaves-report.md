# Leaves lane report

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-leaves`.

## Files

Created `src/client_hooks.rs`, `src/hook_failure.rs`, `src/managed_source.rs`, `src/package.rs`, `src/sharing_hold_mapping.rs`, `src/turn_check.rs`, `tests/t2_leaves.rs`. Modified `src/lib.rs`, `src/fs/descriptor.rs`. No new crate.

## Public API and future wiring

- `client_hooks`: `parse`, `encode`, `entry`, `equal`, `locate`, `merge`, `remove`, `python_era_entries`, `rust_era_command`; `EVENT`, `MAX_CONFIG`. Python-era Claude/Codex commands remain separate from native command construction.
- `hook_failure`: `paths`, `validate`, `read_raw`, `read_one`, `read`, `write`, `record`, `clear`, error and schema/limit constants.
- `managed_source`: `payload`, `verify_index`, `verify_payload`, `verify`, `FileEntry`, `Payload`, result structures, error and limits. `Installation` must be implemented by installation and registered with `register_installation`; the implementation's `source_info` must verify the actual payload before claiming clean ownership.
- `turn_check`: `requests`, `notice_required`, `check`, `combined`, `exit_code`, `failure_notice`, `emit`. Implement/register `ScopeBinding` in scope_binding, `MemoryRefresh` in memory_refresh, `TurnSources` and its locked `NoticeStore` in turn_sources. The store must provide bounded reads, source barrier and namespace lock, enrollment rechecks and durable mutations. Missing registrations refuse.
- `sharing_hold_mapping`: `current_rows`, `source_index`, `successors`, `describe`, `validate_resolution`, `qualified_current`, `cached`, `Rows`, `SourceIndex`. Implement/register `HoldCauses` in sharing_holds, `SemanticSharing` in semantic_sharing, `SecurityScan` in security; missing registrations refuse.
- `package`: committed skill, runtime-compatibility and migrations declarations included with `include_str!`. Baseline `__init__.py` at this snapshot contains no version/compatibility constants; it computes optional Python engine-generation data instead.

## T0 changes

`Descriptor::entries_bounded(limit) -> io::Result<Option<Vec<OsString>>>` shares the existing `readdir` loop with `entries`, stops once limit is exceeded and preserves sorted order. `bounded_directory_entries` covers empty, zero limit, exact limit and overflow.

## CLI declarations

None of these baseline modules declares an argparse CLI. `turn_check::emit` returns status and output for a caller rather than performing CLI I/O.

## Obligations

Fully ported into `tests/t2_leaves.rs`: `test_client_hooks.py::ClientHookContract.test_owned_entry_roundtrip_preserves_foreign_edits`, `test_unowned_collision_and_mutated_or_duplicate_owned_entries_refuse`, `test_ambiguous_json_is_refused`, `test_stop_shape_and_json_boolean_are_not_coerced`.

Partial only: compact-turn request and hook metadata validation have a small direct test, not full baseline integration. All `test_compact_turn_check.py` obligations await turn_sources, scope_binding, memory_refresh and CLI wiring. All `test_hook_failure.py` end-to-end hook/filesystem obligations await full port of associated harnesses and dedicated filesystem test coverage. `test_managed_source_bounds.py`, `test_managed_source_ownership.py`, and managed-source installer/doctor obligations need direct real-Git filesystem integration tests and installation/doctor modules. Known-hold resolution tests await sharing_holds, semantic_sharing and security integration and dedicated mapping tests. These are not claimed as ported.

## Declared deltas and known gaps

Strict UTF-8 and codec nesting bounds follow the Rust codec contract. Malformed mapping structures return typed refusals rather than incidental interpreter exceptions. `turn_check` completion callbacks currently expose only ContractError, so generic non-contract exceptions cannot yet be classified separately from contract errors as in the baseline. Timestamp parser attempts ISO forms beyond RFC3339, but exhaustive equivalence with Python `datetime.fromisoformat` is unverified. Do not treat this report as full behavioral equivalence. Full real-filesystem/process port tests for hook_failure and managed_source are still needed.

## Commands and results

`cargo fmt --all` succeeded. `cargo fmt --check` succeeded. `cargo check --offline --all-targets` succeeded without warnings in owned files; seven existing unused-import warnings in `tests/t1_a2.rs`. `cargo test --offline --test t2_leaves` succeeded three consecutive times, 8/8 each. Earlier runs succeeded 6/6 and 7/7 three times each before additional tests were added. No Python, network or Git writes were used.
