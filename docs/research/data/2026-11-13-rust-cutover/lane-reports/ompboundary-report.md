# OMP boundary lane report (omp_boundary, claude_turns)

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-ompboundary`, detached at integration `681c3b7`. No Git writes, no Python executed, no sub-agents, no awa/Vox/GitHub. Read: lane brief, cutover design (incl. "Harness integration"), ADR-0004, ADR-0029 references, lane reports `ompinstall`, `turn-sources`, `private-state`, `installation`, `cli`, `scope-binding`, `lrs`, `test-world`; baseline `omp_boundary.py` (920 lines) and `claude_turns.py` (36 lines) completely; `tests/contracts/test_omp_boundary.py` and the relevant parts of `omp_stream.py`, `omp_transport.py`, `installation.py`, `private_state.py`, `turn_sources.py`, `uninstallation.py`, `updating.py`.

## Files

Base: started at `681c3b7`; then, per Main, every file changed between `681c3b7` and integration `46a0100` (lifecycle merge, 34 paths) was refreshed from `git -C /opt/agent-memory-worktrees/rust-v015-int show 46a0100:<path>` and my edits re-applied on top; then the omp_stream commit `e6d1348` paths Main named (`src/omp_stream.rs`, `src/omp_stream/*.rs`, `tests/t2_omp_stream.rs`, `src/process.rs`, `src/host.rs`, `src/recall_catalog.rs`, `tests/t2_recall_catalog.rs`, `src/bank_transactions.rs`, `src/error.rs`, `src/lib.rs`, `src/cli/mod.rs`, `tests/wiring-pending.txt`, `tests/obligations/python-baseline-v1.json`) were copied from `e6d1348` and my edits re-applied again (ledger re-marked with `am-dev`, not patched). Shared-file diffs below are against those commits.

Created (owned):
- `src/omp_boundary.rs` — baseline `omp_boundary.py`, every function and branch.
- `src/claude_turns.rs` — baseline `claude_turns.py` (`complete`, `pending`).
- `src/cli/helpers/omp_boundary.rs` — `am omp-boundary` parser/handler (incl. `--stream`).
- `tests/t2_omp_boundary.rs` — 17 tests (14 active, 3 ignored with reasons).

Modified shared files (append-only or Main-approved):
- `src/lib.rs`: `pub mod claude_turns;`, `pub mod omp_boundary;` (alphabetical).
- `src/cli/helpers/mod.rs`: `pub mod omp_boundary;`.
- `src/cli/table.rs`: `route("omp-boundary", &helpers::omp_boundary::HELPER)` at its MEMORY_ROUTES position (before `private`).
- `src/cli/mod.rs` `register_providers` (appended at end): `crate::installation::seams::register_omp_boundary(Some(Arc::new(crate::omp_boundary::InstallationBoundary)))` through `wired(...)`, and `crate::omp_boundary::register_omp_stream(Some(Arc::new(crate::omp_boundary::InstalledStream)))`.
- `tests/wiring-pending.txt`: removed `installation::seams::register_omp_boundary waits for omp_boundary` (now wired); appended `omp_boundary::register_omp_transport waits for omp_transport`. (`register_omp_stream` is wired, so it has no pending line.)
- `tests/obligations/python-baseline-v1.json`: one id marked `partial` via `am-dev obligations mark` (below).

Installation edits (Main-approved; every changed line):
- `src/installation.rs`: `pub(crate) mod seams;` and the `filesystem::{native_rename, replace_record, sync_dir}` re-exports already came with lifecycle at `46a0100`; my separate end block adds only `pub(crate) use transaction::verify_rolled_back_targets;` (+ one comment line).
- `src/installation/transaction.rs:577` `fn verify_rolled_back_targets(…)` → `pub(crate) fn verify_rolled_back_targets(…)` (rustfmt split the signature over 5 lines).
- `src/installation/seams.rs:70-73` (seam change, Main decision): `fn native_transition(&self, plan: &mut Value, check: bool) -> Result<Value>;` and `fn native_activation(&self, plan: &mut Value, check: bool) -> Result<Value>;` each with a one-line doc comment. Baseline mutates the plan in both (`actions` + `omp_disabled_backup`; check-mode `native_layout_quiescence_overrides`).
- `src/installation/transaction.rs` `install` (lines ~877-911): `let mut plan_value`; `let omp_transition = plan.omp_transition;`; transition passes `&mut plan_value` to `native_transition` and `native_activation`, stores the transitioned plan in a `RefCell<Option<Value>>`; the body applies it with the new `apply_native_transition(plan, &mutated)` before `install_now` (drops the extension action when `after` is null, else `before = null, changed = true`; sets `plan.omp_disabled_backup`) — exactly the baseline mutation, so `prepare_lifecycle` and the receipt `backups` see it.
- `src/installation/transaction.rs` new `fn apply_native_transition` (after `install`).
- `src/installation/transaction.rs` receipt build (~1157): `backups` takes `plan.omp_disabled_backup` (any operation, as the baseline `plan.get('omp_disabled_backup')`), no longer only inside uninstall fields; finish (~1497): `if plan.omp_transition || plan.omp_binding.is_some()`.
- `src/installation/plan.rs`: `UninstallFields` loses `omp_transition`/`omp_disabled_backup`; `Plan` gains (appended at its end) `pub omp_transition: bool` and `pub omp_disabled_backup: Option<Value>` (baseline plan keys, set for update and uninstall); `prepare` initializes them `false`/`None`; `Plan::value()` emits `omp_transition: {"writers_quiesced": true}` when set and `omp_disabled_backup` when present (read by `native_transition`/`finish_native_transition`).
- `tests/t2_installation.rs:2607-2612`: `FixtureOmp` adapted to the new seam signatures (`&mut Value`, `Result<Value>`), no behaviour change.

Lifecycle edits (Main-approved, on `46a0100` content):
- `src/updating.rs`: `plan.omp_transition = request.omp_writers_quiesced;` replaces the old `plan.uninstall = Some(UninstallFields { …, omp_transition: true, … })` (update plans no longer carry fake uninstall fields); check mode passes `&mut plan_value` to `native_transition` and records its result as `summary["native_transition"]` (baseline `summary['native_transition'] = native_transition(plan, check=True)`); `native_activation(&mut plan_value, true)`; unused `UninstallFields` import removed.
- `src/uninstallation.rs`: `prepare` builds `UninstallFields` without the moved fields and initializes `Plan.omp_transition/omp_disabled_backup`; `run` sets `plan.omp_transition = request.omp_writers_quiesced` and passes `&mut plan_value` to the check-mode `native_transition`; `uninstall_value` no longer pushes `omp_transition` (now emitted by `Plan::value()`).
- `tests/t2_lifecycle.rs`: appended `RecordingOmp` seam recorder and `writers_quiesced_checks_hand_the_plan_transition_to_the_native_boundary` (update `--check --omp-writers-quiesced` records `native_transition` and passes `omp_transition: {"writers_quiesced": true}` with no uninstall keys; uninstall check passes the same with its uninstall keys; no filesystem change).

## Public API

`agent_memory::omp_boundary`:
- Constants `HISTORICAL_CAPABILITY`, `CAPABILITY`, `MARKER_SCHEMA`, `BINDING_SCHEMA`, `REGISTRY_SCHEMA`, `DRAIN_SCHEMA`, `STREAM_DRAIN_SCHEMA`, `TRANSITION_SCHEMA`; bounds `MAX_RECORD` (4096), `MAX_REGISTRY` (1 MiB), `MAX_REPOSITORIES` (1024), `MAX_NAMESPACES` (65536), `MAX_DRAIN_FILES` (262144), `MAX_QUIESCENCE` (65536), `MAX_INPUT` (65536).
- Baseline → Rust: `_base` → `base(repo)`; `_base(…, enrollment_context=)` → `enrollment_base(repo, context, runtime_root)`; `_marker` → `marker(base)`; `verify_marker` → `verify_marker(repo, runtime_root) -> PrivateEnrollmentContext`; `_observe_marker` → `observe_marker(repo, held: Option<&dyn HeldSource>, runtime_root, body)`; `observe_marker_operation(...)` (the OmpActivation half; callback Value `{root, bank, project_id, scope, held}`, agreed with OmpStream); `_activate_marker` → `activate_marker(repo, fresh_only, runtime_root)`; `stream_startup(repo, session, instance_id, lifecycle, receipt, runtime_root) -> Option<Value>`; `_registry` → `registry()`; `record_bank`; `_bank_binding` → `bank_binding(base)`; `_has_native_source_state` → `has_native_source_state(base)`; `_source_state_bindings` → `source_state_bindings(installation, registered)`; `source_capability_repositories(installation)`; `_omp_index` → `omp_index(repo, runtime_root)`; `_drain_inventory` → `drain_inventory(repo, runtime_root) -> (Value, digest)`; `_shared_native_source_evidence` → `shared_native_source_evidence(repo, base, runtime_root)`; `required_source_capabilities(repo, runtime_root)`; `_stream_proof`, `_verify_stream_drain`, `_read_only_transport_lock` (private); `drain_source_capabilities(repo, quiesced, check, runtime_root)`; `_enrolled_repositories` → `enrolled_repositories(installation)`; `native_activation(plan: &mut Value, check, runtime_root)`; `complete_stored_source_activation(state, runtime_root)`; `native_transition(plan: &mut Value, check, runtime_root)`; `finish_native_transition(plan)`; `recover_native_binding(state, runtime_root)`; `native_recovery_expected(state, target)`; `main` → `command(&Request, input: &mut dyn Read) -> CommandResult` and `stream(argv, prog)`.
- `InstallationBoundary`: implements `installation::OmpBoundary` (runtime root = registered `cli::pack_root`). Wired.
- `agent_memory::claude_turns::{complete(repo, value, runtime_root), pending(repo, session, prompt, (offset, legacy_offset, enrollment_offset), runtime_root)}`.

## Registration traits

- New `omp_boundary::OmpTransport` (`pending_inventory`, `all_segments`, `max_payload` = baseline `MAX_PAYLOAD`, `list_pending`, `source`, `complete`) + `register_omp_transport` / `omp_transport_provider`. Implementer: `omp_transport`. Missing → `Kind::MissingProvider` refusal.
- New `omp_boundary::OmpStream` (`drain_state(repo, base, read_only, runtime_root)`, `main(argv, prog)`) + `register_omp_stream` / `omp_stream_provider`. Implemented and wired: `omp_boundary::InstalledStream` — `drain_state` → `omp_stream::drain_state(&omp_stream::Installed::new(runtime_root), repo, Some(base), read_only)`; `main` → `omp_stream::main(argv, prog, &BoundaryStartup)`, where private `BoundaryStartup` implements `omp_stream::StreamStartup` by calling `omp_boundary::stream_startup` unchanged. OmpStream owns `impl omp_installation::HeldSource for turn_sources::SourceBarrier`.
- Reused `bank_writes::RecoveryFrontier::validate_quiescence` (recovery_frontier) for layout-held native activation; no new frontier trait.
- Implemented: `installation::OmpBoundary` (all seven methods).
- Halves provided, composite still pending:
  - `omp_installation::OmpActivation` / `installation::OmpHarness`: `observe_marker` half = `omp_boundary::observe_marker_operation`. `diagnostics` needs `omp_transport` + `omp_stream`; `instruction_uri` needs `omp_projection`. Pending line `omp_installation::register_activation` unchanged (still waits for omp_projection, omp_stream, omp_transport).
  - `private_state::PrivateStateHooks`: native half = `registry`, `source_state_bindings`, `has_native_source_state`, `shared_native_source_evidence`, `required_source_capabilities`. Still waits for `admission` (`read_input`); pending line unchanged.

## CLI

`am omp-boundary`: baseline parser (`--repo` required, `--stdin`, `--list-pending`, `--offset` int, `--source`, `--session`, `--lifecycle`, `--quiet`, `--refresh`; description = module docstring) plus the finite-stream options `--stream`, `--instance`, `--artifact`, `--omp-agent-dir/--agent-dir` (dest `omp_agent_dir`). `--stream` anywhere → `omp_stream.main` with the stream argv (boundary-only options passed through so the stream parser refuses them); without `--stream` the stream options give argparse `unrecognized arguments: …` (exit 2). Every domain refusal → baseline fixed failure JSON (`{"verified": false, "status": "pending", "canonical_writes_performed": false, "reason": …}`, exit 1) or `am: <reason>` on stderr with `--quiet`. Success → `turn_check::emit`. A nested route was not usable: the surface requires nested route names to be subcommands.

## Tests (`tests/t2_omp_boundary.rs`)

Active: exact marker tokens / malformed markers; exact bank-binding namespace, redirection and project-id refusals; registry absence and every unverifiable shape (incl. >1024); saved-binding discovery and loss (discovery half of the protected-root contract); orphan native state/pending/namespace/saved-root refusals; `stream_startup` outside a repository (absent) and under a broken `.git` ancestor (pending refusal); `finish_native_transition` exact completed bytes and idempotence; `recover_native_binding` journal validation, committed-uninstall completion, foreign receipt refusal, `native_recovery_expected` absence; `native_transition` on a valid saved native receipt with no enrolled banks: check plan (no writes, plan unchanged), real disable (owned extension renamed to the journaled `.agent-memory-disabled-<id>-<uuid>.ts.disabled`, journal `disabled`, exact `actions` rewrite and `omp_disabled_backup`), retry reusing the journaled backup and dropping an after-less action, `finish_native_transition` completion, and a modified extension refused untouched; lifecycle entry refusals/early returns (`native_transition`, `native_activation` with plan untouched, `complete_stored_source_activation` not-required, unquiesced drain); `claude_turns` refusals; `command` fixed failure (JSON and quiet, oversize input); the real `am omp-boundary` binary through the test World (failure JSON, quiet, refresh, `--offset x`, stream-only option, missing `--repo`; `--stream` reaching the real `omp_stream::main`, which ends today at its observer-binding step with the typed `OMP activation interface is not registered; native ownership verification refuses`, exit 1, no stdout — this is before the transport step, because `omp_stream::main` calls `omp_installation::observer_binding` first and the OmpActivation composite is not registered; and `--stream` argv decided by the stream parser: missing stream options exit 2 with its usage); typed missing transport/stream registrations at the library level.

Ignored (bodies complete, real dependencies missing): `record_bank_registers_once_and_refuses_a_moved_private_base`, `required_capabilities_follow_marker_binding_and_stream_evidence` (need `security` registered: `repository_config` bank path), and the obligation test below.

## Obligations

- Partial (marked): `tests/contracts/test_omp_boundary.py::NativeSources.test_protected_private_root_cannot_silently_move_or_lose_binding` — Rust test `protected_private_root_cannot_silently_move_or_lose_binding` (ignored: needs `security`; native state is the activated binding/marker/registry written as fixture bytes, the baseline observe step needs `omp_transport`); its discovery half runs actively in `saved_native_binding_discovery_refuses_a_lost_binding`.
- Deferred, unwritten (need `omp_transport` observe/complete/pending inventory, plus `omp_stream`, `omp_control`, `omp_projection`, `recovery_frontier`, `security`, `admission`, `installation` end-to-end, `uninstallation`/`updating`): all other `test_omp_boundary.py` ids — `NativeSources.*` (11 others), `StoredBatchControls.*`, `OwnedInstructionBoundary.*` (20), `ExplicitNativeOwnership.*` (4), `NativeLifecyclePublic.*` (6), `NativeSmokeQualification.*` (7); and the omp_boundary-owned private-state deferrals `test_private_observer_interrupts.py::PrivateObserverInterrupts.{test_transport_absence_rechecks_normal_and_ordinary_failures,test_transport_absence_preserves_interrupts_and_fatal_failures,test_public_native_drain_preserves_interrupts_and_releases_source_barrier}` (need transport/stream + private-state hooks). The `OwnedInstructionBoundary` ids listed in ompinstall-report need omp_projection/omp_transport/omp_stream; none can honestly run now.

## Implementation completeness

Every baseline function and branch is ported. Paths that need an unported callee refuse with a typed missing-provider error at runtime: `drain_inventory`, `required_source_capabilities` with markerless evidence, `drain_source_capabilities`, `native_transition` drains (transport, and the stream inventory through omp_stream's installed authority, which itself needs omp_transport); layout-held `native_activation` (recovery_frontier); everything through `scope_binding::private_enrollment_context` (`activate_marker`, `verify_marker`, `observe_marker`, `omp_index`, `stream_startup` in a bank) needs `recovery_frontier` registered; `bank_writes::bank_path` needs `security`; `observation_barrier` (check modes) needs `PrivateStateHooks`; `turn_check::check` (`--refresh`) needs `memory_refresh`. CLI `--stream` runs the real `omp_stream::main` via `InstalledStream`; it stops at `omp_installation::observer_binding` until the OmpActivation composite (installation + omp_boundary halves, plus omp_projection/omp_transport) is registered.

## Declared deltas

- Held source barrier in `observe_marker`: the baseline checked the concrete `_HeldPrivateLock` type, its path and registry identity. The Rust interface passes `&dyn HeldSource`; the port requires that this thread holds the live barrier registered for the exact base (`turn_sources::source_barrier_held`) and calls `held.recheck()`. Forgery is prevented structurally by the trait implementer (turn_sources' `SourceBarrier`).
- `observe_marker` rechecks marker/binding/registration at exit also after an operation refusal (frozen OmpActivation contract); private enrollment recheck as `scope_binding::private_enrollment_reads`.
- Directory listings are sorted (index `pending/`, `writes/`, shared namespaces). Baseline `_omp_index` pin order followed `os.scandir` order (filesystem-dependent), so a Python-written drain proof digest can differ from Rust's for the same evidence when scandir order was not sorted; such a bank re-drains (the proof is a pending requirement, never inferred completion).
- Missing records that the baseline read via `FileNotFoundError` (binding, index entry, quiescence attestation, drain-walk file vanishing) refuse with a contract message instead of an `OSError`; all callers already treated both as refusal.
- `native_transition` without an owned target record for the saved extension refuses (`Native binding is modified/unowned; …`) instead of `StopIteration`.
- Installation inventories (`rollback_repositories`, `observed_repositories`, `bank_bindings`, `banks`) must be arrays of strings/objects with `repository`; other shapes refuse (`Unverifiable saved bank binding`) rather than iterating characters.
- Marker creation that finds the name occupied under the exclusive barrier refuses (`Source compatibility marker appeared during activation; preserve it`) — baseline raised `FileExistsError`.
- `--offset` accepts `[+-]?[0-9]+` (Python `int()` also accepted surrounding whitespace and `_`).
- `--stream` options are declared on the boundary parser, so `am omp-boundary --help` lists them; without `--stream` they are refused as unrecognized (exit 2), as before. Stream argv is rebuilt in a fixed order.
- `claude_turns.pending` takes `usize` offsets (negative offsets unrepresentable).
- `native_activation` reads quiescence attestations with turn_sources' private-file reader (baseline `turn_sources.read_bytes`, same policy) bounded at 65536.

## Foundation changes

None.

## Commands (macOS arm64)

Platform: aarch64-apple-darwin, Darwin 27.3.0 (xnu-13432.60.57, arm64), rustc 1.99.1 (58cf8f9c3 2026-10-13), cargo 1.99.1. Linux musl targets were not built here: code uses only the existing foundation (no new libc symbols, no `unsafe`).

Final results, on the tree refreshed to `46a0100` + `e6d1348` paths plus this lane:
- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: no warnings.
- `cargo test --offline --features dev-tools --test t2_omp_boundary` ×3: 14 passed, 3 ignored, each run.
- `cargo test --offline --features dev-tools --test t2_lifecycle writers_quiesced`: 1 passed.
- `cargo run --offline --features dev-tools --bin am-dev -- obligations mark --status partial --ids-file <file>`: 1 id marked (ledger diff vs `46a0100`: that status plus the two counters).
- Panic scan `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/omp_boundary.rs src/claude_turns.rs src/cli/helpers/omp_boundary.rs`: no matches.
- Whole suite `cargo test --offline --no-fail-fast` (after the `e6d1348` refresh and the stream adapter): cargo exit 0; 58 `test result: ok`, 0 `FAILED`; summed 893 passed, 0 failed, 45 ignored (sums include re-executed worker lines). Before the stream adapter, on `46a0100`: 56 results ok, 785 passed, 41 ignored.
- Earlier, before the refresh (on `681c3b7`): whole suite 712 passed, 0 failed, 38 ignored; `wiring` 2 (+1 ignored), `t2_cli` 24, `t2_installation` 53, `t2_omp_install` 20.
