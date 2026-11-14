# Lifecycle lane report (updating, uninstallation, purging, doctor_fix)

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-lifecycle`, detached at `fabc937`, with every non-owned file refreshed from committed integration content (`git -C rust-v015-int show HEAD:<path>`) up to `681c3b7` (turn_sources, layout_history, private_workspace, Intel-macOS drop), my shared-file edits re-applied on top. No Git writes. No network.

## Files

Created (owned):
- `src/updating.rs` — baseline `updating.py` (`am update`) + the production `installation::Updating` implementer `updating::Runtime`.
- `src/uninstallation.rs` — baseline `uninstallation.py` (`am uninstall`).
- `src/purging.rs` — baseline `purging.py` (`am purge-data`).
- `src/doctor_fix.rs` — baseline `doctor_fix.py` (`am doctor --fix` body) + the `Diagnostics` seam.
- `src/cli/helpers/{updating,uninstallation,purging}.rs` — native entries.
- `tests/t2_lifecycle.rs` — 45 tests (42 run, 3 ignored with reasons).

Shared files (append-only):
- `src/lib.rs`: `pub mod doctor_fix;`, `pub mod purging;`, `pub mod uninstallation;`, `pub mod updating;` (alphabetical).
- `src/cli/helpers/mod.rs`: `pub mod purging; uninstallation; updating;`.
- `src/cli/table.rs`: `UPDATE_NATIVE`, `UNINSTALL_NATIVE`, `PURGE_NATIVE` between install and recover (baseline order install, update, uninstall, purge-data, recover).
- `src/cli/mod.rs` `register_providers`: appended `installation::seams::register_updating(Some(Arc::new(updating::Runtime::process())))`.
- `tests/wiring-pending.txt`: removed `installation::seams::register_updating waits for updating`; added `doctor_fix::register_diagnostics waits for diagnostics`.
- `tests/obligations/python-baseline-v1.json`: 51 ids marked `ported`, 14 `partial` via `am-dev obligations mark`.
- Installation (Main-approved, visibility only, no behaviour change): `src/installation.rs` `mod seams` → `pub(crate) mod seams` (so the composition root can call `installation::seams::register_updating`, the path `tests/wiring.rs` expects) and one appended `pub(crate) use` block (`filesystem::{is_directory, is_regular, lexists, named, native_rename, read_optional, replace_record, sync_dir}`, `plan::unchanged_parent`, `transaction::{receipt_committed, require_replannable_journal}`); `src/installation/transaction.rs`: `receipt_committed`, `require_replannable_journal` → `pub(crate)`.

Foundation changes: none.

## Public API

`updating`: `RECORDED_GUIDANCE`, `LIFECYCLE_COMMANDS`, `READBACK_TIMEOUT` (30 s), `MAX_READBACK_OUTPUT`, `MAX_ROUTER`, `MAX_REPOSITORIES`; `VersionKey`, `version_key`, `configuration`, `current`, `qualified_current`, `router_metadata`, `banks`, `release_transition`, `declared_commands`, `compatible_surface`, `acquire_candidate`, `staged`, `wrapper_readback`, `UpdateRequest {check, repo, omp_writers_quiesced, omp_peers_upgraded, omp_layout_quiescence, layout_quiescence, bootstrap_binary}`, `run`, `update_command`, `Runtime {new, process}` (implements `installation::Updating`). Crate-internal: `merged` (`{**a, **b}`), `printed` (baseline `execute` print/exit rule, shared by update/uninstall/purge).

`uninstallation`: `REPAIR`, `reinstall`, `without_block`, `prepare`, `preserved_reader`, `UninstallRequest {check, omp_writers_quiesced, preserve_capable_runtime, layout_quiescence}`, `run`, `uninstall_command`.

`purging`: `SCHEMA`, `MAX_RECORD` (32 MiB), `MAX_FILE`, `MAX_ENTRIES`, `MAX_BYTES`, `MAX_LOCK`, `MAX_ROUTER`, `LOCK`, `OPERATION`, `SCOPE`, `REPAIR`, `canonical`, `PurgeRequest {repo, path, check, confirm}`, `run`, `purge_command`.

`doctor_fix`: `TARGET_COMPONENTS`, `RECREATABLE`, `MAX_MARKER`, trait `Diagnostics {run, report}`, `register_diagnostics`, `diagnostics_provider` (pub(crate)), `trusted_receipt`, `reconcile_hooks`, `recover_completed_bank`, `run`.

## Update design (Rust-to-Rust, signed release-v2 only)

- Probe the signed manifest (`release::probe_with` over `Services.release`). If manifest digest, revision, version and `targets[TARGET].artifact_digest` equal the receipt and `current()` verifies, plan against the installed runtime (no archive or binary fetch); otherwise `acquire_candidate`: signed pack (`acquire_with`) + `am-<triple>` and `am-<triple>.sshsig` from the tag-pinned base, `verify_signature_with(…, agent-memory.release-binary, signers)`, size and sha256 bound to the manifest, written as `bin/am` 0755. `--bootstrap-binary PATH` (Main's requirement for the re-run bootstrap) instead calls `installation::stage_release`, binding version/size/sha256 of the bytes at PATH and installing exactly those bytes, never re-downloading.
- `staged`: `installation::source_info(release)` (`installed_with` + artifact inspection, digest equality), `release_transition` (downgrade refused; same version must keep `artifact_digest` (per-target runtime digest), `revision`, `release_manifest_sha256`), `compatible_surface` (command names from the candidate's and the current runtime's *signed* `command_surface`, validated by `cli::surface::Declared::from_value`; `{update, install, recover} ∪ existing ⊆ candidate`). The candidate is never executed before activation.
- Activation: installation's `prepare(candidate_root)` plans the runtime root as a `source` action; `install()` stages the copy and swaps it with `renamex_np(RENAME_SWAP)`/`renameat2(RENAME_EXCHANGE)`; the old root stays as backup (a running `am` keeps its inode). Before/after hooks re-run the bank scan; after activation `wrapper_readback` executes each owned command link with `--help` and `update --help` (30 s deadline).
- Quiescence: `--omp-peers-upgraded` feeds `configuration`; `--omp-layout-quiescence`/`--layout-quiescence` go into the plan; `--omp-writers-quiesced` sets the plan's OMP transition.
- Locking/recovery: `runtime_faults::operation("update", paths)` + `installation::locked` + `recover(for_install)` exactly as install; `runtime_faults::response("update")`.
- Refusals: v1/v2 receipts → `installation::ADOPTION_GUIDANCE` (before preflight/probe); managed source → `installation::MANAGED_GUIDANCE` (before preflight/probe/staging); development → `RECORDED_GUIDANCE`; release-v1 manifest → `release_distribution::LEGACY_GUIDANCE`.

## Registration traits

- Implemented: `installation::Updating` by `updating::Runtime` — wired.
- Introduced: `doctor_fix::Diagnostics` (`run(operation, repo, projection)`, `report(operation, component, state, detail, repair)`), future implementer `diagnostics`; pending line added.
- Reused (no new trait): `bank_writes::recovery_frontier_provider()` (`observe_owned`, `verified_hold`), `bank_writes::turn_sources_provider()` (`canonical_writer_barrier`), `installation` seams `PrivateState`, `OmpBoundary`.

## Production status of each command (honest)

- `am update`/`am uninstall`: implementation complete; at runtime they need installation's `PrivateState` seam (pending: "blocked by private_state::activation_guard …") and, for any change requiring a bank scan, `private_state` hooks (pending: installation, omp_boundary, admission) — both are typed refusals until registered. Same as `am install` today.
- `am purge-data`: refuses with `Repository boundary security scan provider is not registered` (repository_config bank locator) and then `recovery_frontier provider is not registered` until `security` and `recovery_frontier` are ported.
- `am doctor --fix`: `doctor_fix::run` refuses `Error::Unregistered("Diagnostics (doctor report)")` until `diagnostics` exists; `doctor` itself is diagnostics' native entry (not registered here).

## CLI declarations

- `update` (help "Check or activate a verified managed-channel update"): `--check` store_true; `--repo` append default []; `--omp-writers-quiesced`, `--omp-peers-upgraded` store_true; `--omp-layout-quiescence` append (default None); `--layout-quiescence` append default []; new `--bootstrap-binary PATH`.
- `uninstall` (help "Remove only unchanged owned runtime targets; preserve project memory"): `--check/--dry-run`; `--omp-writers-quiesced`; `--preserve-capable-runtime`; `--layout-quiescence` append default [].
- `purge-data` (help "Explicitly preview or confirm deletion of one exact local project bank"): `--repo` required; `--path` required; required exclusive group `--check` | `--confirm`.

## Obligations

Ported (51): update `test_current_signed_update_checks_manifest_without_archive_or_bank_scan`, `test_read_only_check_valid_update_and_repeat_preserve_user_text_and_source_channel`, `test_dangling_lifecycle_records_are_preserved_and_refuse_update`, `test_development_sources_are_never_mutated_by_update`, `test_lost_response_and_partial_activation_recover_then_retry_without_duplicates`; lost responses `test_update_response_is_lost_after_real_new_revision_is_active`, `test_uninstall_response_is_lost_after_runtime_is_removed_and_data_retained`; all 16 `test_update_write_{before,after}[_part_N]` (8 partitions each, step % 8) and all 16 `test_uninstall_write_{copy,link}_{before,after}[_part_N]` (4 partitions, step % 4); uninstall `test_preview_remove_repeat_and_reinstall…`, `test_modified_copy_and_block_are_preserved…`, `test_user_added_blank_lines…`, `test_allowed_outside_edits_survive…`, `test_missing_managed_source_is_removed…` (runtime root), `test_interrupted_removal_recovers…`, `test_lost_uninstall_success_response…`, `test_missing_receipt_refuses…`, `test_preview_refuses_a_receipt_that_conflicts…`, `test_development_source_is_preserved`; legacy `test_changed_payload_is_preserved`, `test_untracked_payload_is_preserved`.

Partial (14) — what is missing:
- `test_runtime_update::test_current_update_checks_ownership_without_scanning_banks`, `test_same_version_still_reconciles_changed_owned_receipt`: the console human line ("am X is up to date.") is not asserted (console render of update is the CLI lane's, tested there with documents).
- `test_downgrade_non_fast_forward_and_invalid_candidate…`: downgrade, same-version identity change, surface loss and unsigned/unbound binary ported; non-fast-forward and `python_minimum` cases are obsolete (no Git channel, no interpreter); missing-compatibility-schema candidate not exercised.
- `test_dirty_remote_ref_and_unowned_target_changes…`: copy-edit, block-edit and runtime-root edit ported; dirty/remote/branch are Git-channel cases (managed refused).
- `test_runtime_uninstall::test_shared_hooks_actual_uninstall_and_committed_recovery`: codex preset, direct and interrupted-after-receipt ported; claude preset and the edited-after-interrupt recover/reinstall branch not ported.
- `test_legacy_uninstall_metadata::test_metadata_drift_removes_verified_owned_source`: removal of the adopted legacy tree ported; real `.git` metadata drift requalification (`managed_source::verify`) not exercised (fixture trees are opaque, not Git).
- `test_runtime_purge::test_broad_home_root_and_symlink_inputs…`: home/root/alias ported; broad and inside-link wait for security/recovery_frontier (ignored test holds the broad case).
- `test_runtime_purge::test_exact_preview_confirmation…`, `test_interruptions_before_journal_detach…`, `test_lifecycle_lost_responses::test_purge_response_loss…`: ignored tests (`needs security, recovery_frontier registered`).
- `test_doctor_fix::{test_missing_owned_runtime_targets_are_recreated, test_noop_and_restore_missing_owned_hook, test_modified_owned_hook_is_preserved_and_not_replaced}`: reconciliation through `trusted_receipt`/`reconcile_hooks` ported; the `doctor --fix` report wrapper waits for diagnostics. `test_healthy_unrelated_hook_configuration_needs_no_repair`: ignored (`needs diagnostics registered`).

Deferred (unported, waiting): remaining `test_runtime_update` (`test_changed_owned_copy_update_retains_prior_payload…`, `test_competing_update_observes_the_live_lifecycle_lock`, `test_discovered_and_explicit_banks…`, `test_fast_forward_outside_the_pack…`, `test_trusted_source_helper_updates…`, `test_unchanged_channel…umask`, `test_unchanged_revision_refreshes_allowed_outside_text…`), `test_update_discovery_gaps.*`, `test_update_mutable_*`, `test_update_parallel_inventory.*` (real `banks` needs private_state hooks: installation, omp_boundary, admission), remaining `test_runtime_purge.*`, `test_purge_{write,unlock}_*` (security, recovery_frontier), remaining `test_doctor_fix.*` (diagnostics), `SignedReleaseLifecycle` one-liner/v002 cases (release qualification), `test_lifecycle_lost_responses::test_recovery_response_is_lost…` (installation recover, not claimed here).

Tests that use seam fixtures (same pattern as `t2_installation`): `FixturePrivateState` (installation `PrivateState`) and `LifecycleUpdating` = the real `updating::Runtime` with only `banks` replaced (private_state hooks pending); artifacts via `artifact::inspect_runtime_with(…, FixtureCatalog)`. Signed releases are built per test with a throwaway ed25519 key, `file://` assets, `release::encode_archive`, the real `cli::surface::declaration()` as `command_surface`; the Rust-to-Rust test installs the built `am` (`CARGO_BIN_EXE_am`) and updates to a second build (same bytes + one byte; release verification binds size and sha256), proving an open handle on the old `bin/am` still reads the old bytes, the runtime inode changed, the command links dispatch the new binary, and the old root is retained as backup.

## Unported baseline branches (listed per the brief)

- `updating.staged` managed-channel branch (`git ls-remote`, `git clone`, fast-forward check, managed `python_minimum`) — managed channels refuse with `MANAGED_GUIDANCE`.
- `updating.release_transition` same-version *legacy managed* comparison (`git ls-tree`) and the `python_minimum` check — no interpreter; unreachable (legacy receipts refuse).
- `updating.current` managed `origin`/branch checks — managed refused.
- `updating.run` check-mode `summary['native_transition'] = native_transition(plan, check=True)`: the current seam returns `()`; the call is made but its value is not recorded. The OMP-boundary lane is changing the seam to `(&mut Value, check) -> Result<Value>`; updating and uninstallation must then pass the mutable plan value and record/use the result (two call sites: `updating::work`, `uninstallation::work`).
- `uninstallation` managed `source` requalification applies to `legacy-source` targets (the Rust-era home of a managed checkout).

## Declared deltas

- Python-era (v1/v2) receipts: `am update` and `am uninstall` refuse with `ADOPTION_GUIDANCE` (adopt first via the bootstrap); v3 managed refuses with `MANAGED_GUIDANCE` before any staging.
- `compatible_surface` reads the signed manifests' `command_surface` (current runtime and candidate) instead of AST dispatch.
- `wrapper_readback` executes owned `command-link` targets (Rust has no wrapper files); only after activation.
- `banks` inspects repositories sequentially in sorted order (baseline used a 4-thread pool only to overlap IO); `runtime_root` passed to private_state is `<runtime>/skills/agent-memory`. `router_metadata` (baseline `migration.router_metadata`) is ported privately in `updating` because `migration` is not ported; non-UTF-8 routers refuse.
- `reinstall.argv` is `am install …` (release: requires the trusted bootstrap; development: `--development-source PATH`).
- `preserved_reader` counts `command-link` as an invocation route; Rust-era receipts inspect the runtime (pack + binary).
- Purge: walk order is sorted top-down (baseline recursed in readdir order; the confirmation digest is order-independent); `retained.runtime` reports the owned runtime root; the new-lock release is path-based exactly like `local_registry.exclusive`'s exit (not the descriptor-based Rust helper, which would unlink a detached quarantine's journal-bound lock); `EPERM` on the recorded PID refuses with `[Errno 1] Operation not permitted: pid N`; the alias-unlink "first remaining alias" is the lexicographically first (baseline: arbitrary set order).
- `--omp-writers-quiesced` on update is carried in `Plan.uninstall` (`UninstallFields.omp_transition`, the only plan field installation reads for the OMP transition); the OMP lane's `Plan::value()` `omp_transition` addition supersedes this.
- doctor_fix: `trusted_receipt` compares the executing binary with the receipt's runtime binary (no `artifact.PACK`); `command-link` added to recreatable/target components.

## Commands run (macOS arm64: aarch64-apple-darwin, Darwin 27.3.0, rustc 1.99.1)

- `cargo fmt --check` — clean.
- `cargo check --offline --all-targets --features dev-tools` — no warnings.
- `cargo test --offline --features dev-tools --test t2_lifecycle` — 42 passed, 3 ignored, three consecutive runs on the final tree (85 s, 90 s, 95 s; the update/uninstall partition replays dominate).
- `cargo test --offline --features dev-tools --test wiring --test t2_cli --test t2_installation` — wiring 2 passed/1 ignored (cutover gate), t2_cli 24/24, t2_installation 53/53.
- Whole suite `cargo test --offline --features dev-tools --no-fail-fast` — every target green (31 test binaries, 0 failures), including `obligations_ledger` 5/5, `t2_delivery` 34/34, `t2_installation` 53/53, `t2_lifecycle` 42 passed/3 ignored, `wiring` 2 passed/1 ignored.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/updating.rs src/uninstallation.rs src/purging.rs src/doctor_fix.rs src/cli/helpers/{updating,uninstallation,purging}.rs` — no matches.
- Not run: Linux aarch64 musl (coordinator).

## Process notes

- Once I ran `python3 -` with no input by mistake inside a shell line (empty program; no behaviour inspected).
- Early small edits to shared files used `sed -i`/`printf >>`; after the integration refresh every shared-file change was re-applied with the edit tool (the final diffs are exactly the lines listed above).
