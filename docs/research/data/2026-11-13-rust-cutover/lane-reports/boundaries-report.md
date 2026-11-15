# Boundaries lane report (v0.0.15 Rust cutover)

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-boundaries`, detached at integration `cc85cc37`. No Git writes, no Python executed, no sub-agents, no new crates.

## Files

Created (owned):
- `src/claude_boundary.rs` — port of `claude_boundary.py` (176 lines).
- `src/codex_boundary.rs` — port of `codex_boundary.py` (210 lines).
- `src/cli/helpers/claude_boundary.rs`, `src/cli/helpers/codex_boundary.rs` — parser declarations and typed handlers.
- `tests/t2_boundaries.rs` — the lane test target (10 tests).

Modified (shared, append/insert only):
- `src/lib.rs`: `pub mod claude_boundary;` and `pub mod codex_boundary;`, each in its alphabetical position.
- `src/cli/helpers/mod.rs`: `pub mod claude_boundary;` and `pub mod codex_boundary;`.
- `src/cli/table.rs`: `route("claude-boundary", …)` and `route("codex-boundary", …)` after `admission`, before `omp-boundary` (the doc-comment order).
- `tests/obligations/python-baseline-v1.json`: 5 obligations `unported` → `partial` (via `ledger-mark.sh`).

Not modified: `src/error.rs`, registration traits, `register_providers`, `tests/wiring-pending.txt`. No line in `wiring-pending.txt` waits for `claude_boundary` or `codex_boundary`, and these modules add no seam: every baseline callee (`claude_turns`, `turn_sources`, `turn_check`, `boundary_assessment`, `memory_publication`, `sharing_recovery`, `scope_binding`, `bank_writes`, `trust_paths`, `sharing_limits`, `developer_identity`, `bank_capacity`) is already ported and called directly.

## Public API (for Main to wire the Hooks lane seams at merge)

Baseline boundary functions, baseline names:
- `claude_boundary::complete(repo: &Path, value: &Value, runtime_root: Option<&Path>) -> turn_check::CompletionResult<Value>` — baseline `claude_boundary.complete(repo, value)`. An `observation_id` request goes to `claude_turns::complete`; otherwise the legacy `source_uuid` acknowledgment.
- `codex_boundary::complete(repo, value, runtime_root: Option<&Path>) -> CompletionResult<Value>` — baseline `codex_boundary.complete`.
- `codex_boundary::legacy_complete(repo, value) -> CompletionResult<Value>` — baseline `legacy_complete`.
- `codex_boundary::list_pending(repo, (offset, legacy_offset, enrollment_offset): (usize, usize, usize), runtime_root: &Path) -> CompletionResult<Value>` — baseline `list_pending`.
- `codex_boundary::legacy_list_pending(repo, runtime_root: &Path) -> CompletionResult<Value>` — baseline `legacy_list_pending` (no baseline caller; ported for parity).
- `claude_boundary::command(&Request, &mut dyn Read) -> CommandResult` and `codex_boundary::command(…)` — baseline `main` after argparse; `claude_boundary::Request`, `codex_boundary::Request` are the argparse namespaces.
- `claude_boundary::PageOffset` (`parse`, `is_zero`, `value`): argparse `type=int` page offsets. `claude_boundary::{MAX_RECORD = 4096, MAX_STDIN = 65536}`, `codex_boundary::{MAX_INVENTORY_ENTRIES = 65536, MAX_PENDING = 4096}`.
- Crate-internal: `claude_boundary::acknowledge(repo, value, &Legacy)` is the one legacy acknowledgment shared by both harnesses (Claude/Codex differ only in directory, schemas, bound refusal text, and Codex's extra `session`/`turn` binding).

`runtime_root`: `None` resolves the installed pack via `cli::pack_root()` only when an observation-bound completion needs it. The legacy acknowledgment never reads the runtime pack, as in the baseline. A hook that already holds its runtime root passes `Some(root)`. The return type is `turn_check::CompletionResult`, so the functions plug directly into `turn_check::check` (`|repo, value| complete(repo, value, Some(root))`). `?` converts `CompletionError` into `ContractError` where a caller needs `Result<Value>`.

Hooks lane note: per `hooks-report.md`, the hooks import no `claude_boundary`/`codex_boundary` code, and none of its seams wait for these modules. Nothing needs wiring at merge beyond the table lines above.

## CLI declarations

- `am claude-boundary`: `--repo` (required), `--input`, `--list-pending`, `--session`, `--prompt`, `--quiet`, `--refresh`, `--offset`, `--legacy-offset`, `--enrollment-offset`. Description = the module docstring.
- `am codex-boundary`: `--repo` (required), `--input`, `--list-pending`, `--stdin`, `--offset`, `--legacy-offset`, `--quiet`, `--refresh`, `--enrollment-offset`.
- The `int` options give `argument --X: invalid int value: '<repr>'`, exit 2 (the same form as omp-boundary).

## Obligations

Fully ported: none.

Partial: all 5 are written, carry their obligation line, and are `#[ignore]`d. Each was run with `--include-ignored`; every one fails only at the fixture's `scope_binding::apply` with `MissingProvider: recovery_frontier provider is not registered`.
- `test_claude_boundary.py::ClaudeBoundaryContract.test_bound_source_and_duplicate_are_fail_closed` (needs recovery_frontier). Missing: the `claude_stop` hook calls (first Stop exit 2 with source UUID guidance, repeated/guarded/missing-prompt Stops). The test writes the hook's exact legacy pending record (`json.dumps` default bytes) instead; every `claude-boundary` CLI assertion is ported.
- `test_claude_source_turns.py::ClaudeSourceTurns.test_legacy_unicode_done_is_bounded_and_corruption_is_not_quiet` (needs recovery_frontier). Missing: the Stop-hook quietness and pending report after corruption and FIFO transcript. Added in their place: the CLI retry over the corrupted done record refuses with "Completed boundary differs from requested retry".
- `test_claude_source_turns.py::ClaudeSourceTurns.test_interrupted_old_unicode_completion_retries_without_rewriting_evidence` (needs recovery_frontier). Missing only the hook enrollment step (pending record written directly).
- `test_codex_stop.py::CodexStopContract.test_legacy_completion_remains_quiet_without_rewriting_source` (needs recovery_frontier). Missing: the final `codex_stop` hook call. Added: a wrong-session refusal and `legacy_list_pending` before/after, plus an `already_completed` retry.
- `test_claude_source_turns.py::ClaudeSourceTurns.test_interrupted_prompt_is_discoverable_and_stdin_cannot_claim_publication` (needs security, recovery_frontier, installation). Registration goes through `turn_sources::register` instead of the `claude_prompt` hook. Listing uses `claude_turns::pending` in-process, because the CLI `--list-pending` path needs an installed runtime receipt (`cli::pack_root`). The stdin publication refusal runs through the real CLI.

Deferred (not annotated):
- `test_claude_boundary.py::ClaudeBoundaryContract.test_published_ack_requires_real_authority_readback`: needs semantic_sharing (reconcile review proof), the GitHub publication fixture (private to `tests/t2_publication.rs`) and the `claude_stop` hook.
- `test_claude_boundary.py::ClaudeBoundaryContract.test_uninitialized_and_nonrepository_are_quiet`: it exercises only the `claude_stop` hook, none of this lane's code. It belongs to the Hooks lane, which did not list it.
- `test_compact_turn_check.py::CompactChecks.test_actual_cli_one_call_quiet_and_exact_legacy_receipt_scope`, `…test_batch_distinct_observations_and_partial_failure`: need memory_refresh plus the RefreshFixture/publication fixture.
- `…test_actual_assessment_refresh_failure_dedup_and_recovery`: needs memory_refresh, and monkeypatches `memory_refresh.current_result`; it needs a real failing refresh state, not a fake provider.
- `test_held_source_enrollment.py::HeldSourceEnrollment.test_typed_context_exact_retry_and_no_ack_authority`: needs recovery_frontier held-layout fixtures.
- `test_read_only_audits.py::ReadOnlyAudits.test_direct_hook_and_boundary_helpers_do_not_create_source_caches`: mixes the `codex_prompt` hook and `private_workspace` helper; its `__pycache__` subject has no Rust counterpart. `--help` exit 0 for both routes is covered by `boundary_routes_parse_like_their_baseline_helpers`.
- `test_claude_source_turns.py` `…enroll_before_transcript…` and `…shared_native_prompt…`, and `test_codex_stop.py`'s other three tests: hook-centric, owned by the Hooks lane.

New non-ledger tests (run, green):
- `page_offsets_follow_python_int`: PageOffset accepts Python `int()` forms and refuses invalid ones.
- `claude_route_refuses_mode_mixtures_and_unbounded_stdin`: every mode-mixture refusal, checked against the exact failure envelope bytes; the 64 KiB stdin bound on both channels; the stdin publication refusal; the generic reason for undecodable or missing input.
- `codex_route_keeps_fixed_pending_reason_and_quiet_channel`: the fixed pending reason, and stderr under `--quiet`.
- `boundary_routes_parse_like_their_baseline_helpers`: help, invalid int, required `--repo`, unknown `--stdin` on claude.
- `codex_completion_requires_an_object_before_scope`.
- `claude_route_hides_os_failures_of_callees_behind_the_fixed_reason`: an unreadable developer registry gives the exact fixed-reason envelope, while a readable one keeps its contract reason.

No baseline function or branch is unported. The `partial` status covers only the hook steps and fixture dependencies listed above.

## Declared deltas

1. Error classification (Main-directed). The baseline shows `str(exc)` only for `ContractError`; `OSError`/`ValueError` get a fixed reason (Claude: "Original source or pending record is unavailable or unsafe…"; Codex: always fixed unless the error is bank capacity or an "inventory exceeds" refusal). In Rust:
   - Origin classification in this lane's own files: its descriptor operations, record and input decoding, and input reads map to `CompletionError::Unexpected`, which gives the fixed reason.
   - Callee errors: ported callees still report an OS failure as a plain `Kind::Contract` error carrying the `std::io::Error` text, through their local `io_error` helpers. The Claude route passes every error through **`claude_boundary::classified`** (with `os_error_text`), a single bounded classifier. A `Kind::Contract` message carrying std's `io::Error` OS display signature `(os error N)` becomes `Unexpected` and gets the fixed reason; every other contract refusal keeps its message.
   - **The planned foundation sweep replaces `classified`.** That sweep appends a typed OS kind and moves every `io_error` helper onto it; once it lands, delete `classified`/`os_error_text` and classify on the new kind.
   - Remaining gap until the sweep: a callee's custom `io::Error::new(kind, text)` message (no OS errno) is still shown.
   - Codex is unaffected (its reason is already fixed).
   - Test: `claude_route_hides_os_failures_of_callees_behind_the_fixed_reason` makes the developer registry unreadable (0000) and asserts the exact fixed-reason envelope. It checks the outcome, not the mechanism, so it holds both before and after the sweep.
2. Duplicate JSON keys in the input or in a retained record give the fixed "unavailable" reason, not `Duplicate metadata key: k`. The Rust decoder does not tell decode errors from duplicate-key refusals by kind.
3. Completed-record equality (`load(existing) != record`) is order-insensitive JSON equality through the sorted compact codec (`installation::same`). Python's `True == 1` cross-type equality is not reproduced.
4. Reads use the foundation's pinned reads:
   - `read_prefix(4097)` refuses a record whose identity changes during the read (the baseline read it anyway).
   - `--input` uses `fs::read_source`, which also refuses multi-link files.
5. `legacy_list_pending` differences:
   - It lists entries in sorted order (the foundation's `entries_bounded`), not readdir order.
   - The 65 536-entry bound is checked over the whole directory before records are read.
   - `lexists` treats only ENOENT/ENOTDIR as absent. Any other lstat failure proceeds to the `trust_paths.state` check instead of reporting an empty inventory.
6. The `.done` record is created with an explicit mode of 0600 (`create_exclusive` + `fchmod`), independent of the umask.
7. argparse `type=int`:
   - ASCII forms only (whitespace, sign, single underscores); Unicode digits are refused, as in the hooks lane.
   - Invalid offsets are reported after the required-argument check, and in declaration order, not argv order.
   - Negative or oversized offsets map to `usize::MAX`, so `turn_sources::backlog` gives the baseline "Invalid backlog page offset" (and `claude_turns::pending` "Page offsets select…").
8. `--list-pending` and observation-bound completion resolve the runtime pack with `cli::pack_root()`. An executable without an installed Rust receipt refuses with that provider's message (Claude shows it; Codex shows its fixed reason). The legacy acknowledgment does not need the runtime pack.

## Foundation changes

None.

## Panic audit

`grep -n 'unwrap()\|expect(\|\[.*\.\.' src/claude_boundary.rs src/codex_boundary.rs src/cli/helpers/claude_boundary.rs src/cli/helpers/codex_boundary.rs` → no matches.

## Commands run (aarch64-apple-darwin, Darwin 27.3.0 arm64, rustc 1.99.1)

- `cargo fmt --check` → clean.
- `cargo check --offline --all-targets --features dev-tools` → no warnings.
- `cargo test --offline --features dev-tools --test t2_boundaries`, 3× → `6 passed; 0 failed; 5 ignored` each time.
- `cargo test --offline --features dev-tools --test t2_boundaries -- --include-ignored` → the 5 ignored tests fail only at the fixture bind: `MissingProvider: recovery_frontier provider is not registered`.
- `~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh - tests/t2_boundaries.rs` → `marked ported=0 partial=5` (ledger summary: partial 452, ported 298, unported 1112).
- `cargo test --offline --no-fail-fast --features dev-tools` → exit 0; 72 `test result: ok` lines; no `FAILED`, no nonzero failed count (`t2_boundaries`: 6 passed, 5 ignored).
