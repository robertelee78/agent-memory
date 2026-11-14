# Lane `hooks` report — native `am hook <name>`, adapter retarget, client hook commands

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-hooks`, detached at `8f69d726` (not refreshed to `fba4152d`: this lane uses none of checkpoint/inactive/retrieval/discovery/projections). No Git writes.

## Files

Created:
- `src/hooks.rs` — module root: `NAMES`, `MAX_EVENT`, `MAX_LEDGER_REQUEST`, `Output`, `Runtime`, `input_limit`, `run`; shared helpers (event decode, `git rev-parse` probe with 5 s deadline and 64 KiB capture, `lexists`, fixed outputs, legacy receipt, boundary prompt via `include_str!("../skills/agent-memory/hooks/boundary-prompt.txt")`).
- `src/hooks/turn_hook.rs` (`turn_hook.py`), `claude_prompt.rs` (`claude_prompt.py`), `claude_source.rs` (`claude_source.py`, `pub fn source`), `claude_stop.rs` (`claude_stop.py`), `codex_prompt.rs` (`codex_prompt.py`), `codex_stop.rs` (`codex_stop.py`), `boundary_ledger.rs` (`boundary_ledger.py`, `pub fn reserve`, `pub struct Options`, `pub enum Error`).
- `src/cli/helpers/hooks.rs` — `HOOK_NATIVE`: `am hook {turn-hook,claude-prompt,claude-stop,codex-prompt,codex-stop,boundary-ledger}`; reads stdin up to the hook's bound + 1 byte (64 KiB events; ledger request = CLI input bound).
- `tests/t2_hooks.rs`.

Modified:
- `src/lib.rs` (+`pub mod hooks;`), `src/cli/helpers/mod.rs` (+`pub mod hooks;`), `src/cli/table.rs` (appended `Entry::Native(helpers::hooks::HOOK_NATIVE)` after `doctor`; doc comment extended "then the client hook commands (`hook`)").
- `src/pack_validation.rs` — `python_reference` + inventory check: any `hooks/*.js|*.mjs|*.ts` naming `python3` or a `*.py` file is refused (`dispatch`, "Client hook adapters must invoke the am binary, never Python").
- `skills/agent-memory/hooks/omp.ts` — `Binding.executable` (already emitted by `omp_installation::extension_wrapper`); `supported()` requires it absolute; stream is `spawn(binding!.executable, ['omp-boundary', '--stream', …])`; `pack`/`helper`/`launcher`/`fileURLToPath` removed.
- `skills/agent-memory/hooks/opencode-v2-queue.js` — `runtimeExecutable()` resolves `<root>/bin/am` (release runtime root) or `<root>/target/release/am` (development source) beside its real `<root>/skills/agent-memory`; `execFile(executable, ['hook', 'boundary-ledger'], …)` with the baseline timeouts/maxBuffer; missing binary → `{status:'pending', reason:'ledger-unavailable'}`.
- `opencode.js`, `opencode-v1.js`, `opencode-v2.js`: no Python reference existed; unchanged. Python hook files untouched; nothing in `src`/adapters references them (only `client_hooks::python_era_entries`, the adoption recogniser).
- `tests/obligations/python-baseline-v1.json` (ledger-mark: ported=28, partial=5).

Not changed (already correct on `8f69d726`, verified by tests here): `src/client_hooks.rs` (`rust_era_command` → `<am> hook <name>`, `python_era_entries` exact recognition), `installation::client_hook_entries` (names `turn-hook`/`claude-stop`/`codex-prompt`/`codex-stop`), adoption `require_hook`.

## Public API

`agent_memory::hooks::{NAMES, MAX_EVENT, MAX_LEDGER_REQUEST, Output, Runtime, input_limit, run}`, `hooks::boundary_ledger::{reserve, Options, Error, Result}`, `hooks::claude_source::{source, LIMIT}`, `cli::helpers::hooks::HOOK_NATIVE`.

## Registration traits / seams

None introduced; no `wiring-pending.txt` lines added or removed. The hooks import no `claude_boundary`/`codex_boundary` code. Unported callees are reached through existing seams:
- `published` acknowledgment → `turn_sources::PublicationGate` (`request_value` + `replay`; waits for memory_publication, already listed).
- `refresh_current` combined check → `turn_check::check` → `MemoryRefresh` (waits for memory_refresh, listed).
- Every turn-source operation (register/inspect/pending_notice/reserve_wake/retain_unidentified_input, `private_enrollment_context`) → `bank_writes::RecoveryFrontier` (waits for recovery_frontier, listed). Until it lands, codex/claude hooks over a bank fail closed with their fixed refusal.

## CLI declaration

Native root subcommand `hook` (positional `name`, choices = `hooks::NAMES`), help "Client hook adapter; reads one harness event on stdin". It is a `Native` entry, not a helper `Route`: helper routes must name a baseline `scripts/*.py` wrapper, which hooks never had. The command surface declaration includes it automatically.

Runtime pack: hooks that reach private source state (and Claude/Codex guidance naming `<pack>/turn/GUIDE.md`) use `cli::pack_root()` (the receipt-bound installed runtime). `turn-hook` guidance and `boundary-ledger` do not need it.

## Fault points added (opt-in, token-bound `bank_writes::fault`)

`boundary-ledger-reserve-recheck`, `boundary-ledger-enroll-verify`, `boundary-ledger-wake-lock-reopen` — replace the baseline tests' `os.fsync`/`os.open` monkeypatches.

## Obligations

Ported (28): `test_turn_hook.py` ×5; `test_opencode_boundary_ledger.py` BoundaryLedgerTest ×10, `OpenCodeLauncherContract.test_real_ledger_with_older_python_first` (real `am` binary instead of the python launcher), `UninitializedBoundaryContract.test_absent_context_skips_and_malformed_bank_remains_pending`; `test_opencode_native_boundary.py` NativeLedger ×9 (racing lock creator modelled by a pre-existing competitor lock; lock removal and namespace swaps via fault rendezvous); `test_opencode_queue.py` (stand-in `<runtime>/bin/am` replaces the stub ledger script); `test_hook_failure.py::test_bank_absence_clears_only_its_own_adapter`.

Partial (5, written and `#[ignore]`d): `NativeLedger.test_actual_plugin_enrolls_before_context_queues_once_and_recovers_native_session` (needs recovery_frontier, memory_refresh); `HookFailureContract.test_hook_failure_codes_survive_without_source_content`, `…test_unresolved_identity_retains_fixed_metadata_without_bank_change`, `ClaudeSourceTurns.test_enroll_before_transcript_source_assertion_then_quiet_nullable_stop`, `…test_shared_native_prompt_tracks_three_distinct_observations_without_transcript_projection` (need recovery_frontier; Claude completion uses ported `claude_turns` in place of the `claude-boundary` CLI).

Deferred (left unannotated/unported):
- `PublishedAcknowledgmentContract.test_requires_real_witnessed_publication` — memory_publication + publication fixtures.
- `ClaudeSourceTurns.test_interrupted_prompt_is_discoverable_and_stdin_cannot_claim_publication`, `test_legacy_unicode_done_is_bounded_and_corruption_is_not_quiet`, `test_interrupted_old_unicode_completion_retries_without_rewriting_evidence` — claude_boundary (legacy completion CLI).
- `HookFailureContract.test_source_compatibility_fault_is_not_misclassified_as_identity` — recovery_frontier.
- `HookFailureContract.test_stop_scope_loss_reports_identity_code_without_bank_change`, `test_unexpected_exception_is_fixed_and_does_not_expose_exception_text` — Python monkeypatching of module functions/`RuntimeError`; the Rust hooks have no untyped-exception path (see deltas).
- `HeldSourceEnrollment.test_public_layout_hold_enrolls_real_codex_and_claude_prompt_hooks`, `test_real_codex_claude_hooks_keep_task_active_but_boundaries_pending` — recovery_frontier + layout-recovery held fixtures.
- `CompactChecks.test_native_combined_refresh_uses_protected_writable_state_without_review_echo` — memory_refresh.
- `ReadOnlyAudits.test_direct_hook_and_boundary_helpers_do_not_create_source_caches` — mixes unported codex/claude boundary helpers; its `__pycache__` subject has no Rust counterpart.

New non-ledger tests: real-binary run of every `am hook <name>` (quiet outside a repository, fixed failure form on malformed events, unknown hook = argparse exit 2); uninstalled executable over a bank fails closed (`[boundary/state.unavailable]`, failure codes recorded without prompt content, repository bytes unchanged); captured v0.0.14 Claude/Codex configs: exact Python-era recognition (other pack refused); Python-era → Rust rewrite with unrelated groups/keys/order, non-ASCII key, `1e-05` and a 20-digit integer byte-preserved against a literal expected `indent=2` re-encoding; generated owned commands executed through `/bin/sh -c` reach the native hooks; adapters contain no Python; pack validation refuses Python adapters.

## Declared deltas

1. Boundary guidance is compiled in (`include_str!` of `hooks/boundary-prompt.txt`) instead of read from the pack per event; OpenCode adapters still read the same file.
2. `boundary-ledger` request is bounded by the CLI input bound (baseline `json.load(sys.stdin)` was unbounded).
3. Ledger backlog walks sorted directory names (baseline `scandir` order), so with several faults the first refusal reported is deterministic.
4. Legacy `claude-boundaries` pending count scans at most 1,000,000 entries (baseline unbounded); beyond it the hook refuses.
5. Ledger ownership checks use the effective uid (baseline `os.getuid()` in `boundary_ledger.py`).
6. `AGENT_MEMORY_BOUNDARY_LEDGER_LIMIT` accepts Python `int()` ASCII forms (whitespace, sign, single underscores); Unicode digits are refused (→ `ledger-limit-invalid`).
7. Non-UTF-8, BOM, UTF-16/32 events and lone surrogates are refused at decode, giving each hook's fixed failure output.
8. Codex `internal.unexpected` branches (`except Exception`) have no Rust counterpart: every refusal is typed and takes the classified `[stage/reason]` path.
9. Legacy `claude-boundaries` parents are created 0700 (`fs::private_mkdir`) instead of `Path.mkdir(parents=True)` default modes; `trust_paths.state` is still required.
10. Hooks reaching private source state require the executing binary to be the installed runtime (`cli::pack_root`); otherwise they fail closed in their fixed form (baseline always ran from its pack).
11. The OpenCode adapter locates `am` beside its real pack path (release `bin/am`, else development `target/release/am`); OMP uses the receipt binding's `executable`.
12. `pack_validation` adds the Python-adapter refusal above.

`grep -n 'unwrap()\|expect(\|\[.*\.\.' src/hooks.rs src/hooks/*.rs src/cli/helpers/hooks.rs`: one hit, `codex_prompt.rs:76 [first, rest @ ..]` — a slice pattern, not indexing.

## Commands run (Darwin 27.3.0 arm64, aarch64-apple-darwin, rustc 1.99.1)

- `cargo fmt --check` — clean.
- `cargo check --offline --all-targets --features dev-tools` — 0 warnings.
- `cargo test --offline --features dev-tools --test t2_hooks` ×3 — `35 passed; 0 failed; 5 ignored` each.
- `ledger-mark.sh <names> tests/t2_hooks.rs` — `marked ported=28 partial=5`.
- `cargo test --offline --no-fail-fast --features dev-tools` — exit 0: 69 test binaries, 1069 passed, 0 failed, 99 ignored (log `/tmp/hooks-full-suite.log`).
