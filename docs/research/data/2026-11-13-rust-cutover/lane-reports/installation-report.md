# Installation lane report (Installation-2)

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-install`, synced to committed integration content `f739117` (non-owned files copied from `git show HEAD:<path>`; my edits re-applied on top). No Git writes, no Python executed, no network used by any test.

## Files

Created (owned):
- `src/runtime_faults.rs` — baseline `runtime_faults.py`.
- `src/installation.rs` and `src/installation/{filesystem,records,lock,plan,adoption,seams,transaction,providers}.rs` — baseline `installation.py` (split by concern; every baseline function has a named Rust counterpart, mapping below).
- `src/cli/helpers/installation.rs` — `am install`, `am recover` native entries.
- `tests/t2_installation.rs` — 53 tests.
- `tests/fixtures/v014-state/{all-copy,all-link,claude-opencode-copy,codex-copy-pending-source}/…` + `tests/fixtures/v014-state/derive.sh` (derivation, repeatable; documented in the script header).

Modified non-owned files (Main-authorised; exact diffs are small):
- `src/lib.rs`: `pub mod installation;`, `pub mod runtime_faults;`.
- `src/fs/descriptor.rs` (T0): appended at the end of `impl Descriptor`: `pub fn symlink_at(&self, name: &OsStr, target: &OsStr) -> io::Result<()>` (exclusive `symlinkat`) and `pub fn set_mode(&self, mode: u32) -> io::Result<()>` (`fchmod` on the held descriptor, bits `<= 0o7777` else `InvalidInput`). No tests added in `t0_fs` (exercised by every install test); Main may want a T0 unit test.
- `src/cli/mod.rs` `register_providers`: wires `cli::register_pack_root(RuntimePackRoot)`, `console::register_installed_receipt(InstalledReceiptProvider)`, `cache_recovery::register_installation(CacheInstallation)`, `managed_source::register_installation(&MANAGED_SOURCE_INFO)`.
- `src/cli/table.rs`: `Entry::Native(helpers::installation::INSTALL_NATIVE)`, `Entry::Native(helpers::installation::RECOVER_NATIVE)`; `src/cli/helpers/mod.rs`: `pub mod installation;`.
- `src/console.rs` `bootstrap_command`: rewrite a repeated bootstrap to `update` only when `schema == "agent-memory/install-receipt-v3"` and source kind is `release`/`managed` with the same remote/ref; v1/v2/unknown/unreadable fall through to `install`.
- `src/omp_installation.rs` `validate_receipt`: accepts `agent-memory/install-receipt-v3` (requires `source.target`); v3 is allowed with or without `omp_binding`; v1/v2 rules unchanged.
- `src/artifact.rs` `check_compatibility`: `installation_receipt_schemas` may list v3.
- `skills/agent-memory/schemas/runtime-compatibility.json`: `installation_receipt_schemas` adds `agent-memory/install-receipt-v3`.
- `src/cache_recovery.rs` `qualify_with`: receipt `artifact_files` is the runtime inventory (pack + `bin/am`); requires its `bin/am` entry == `{sha256: targets[triple].sha256, mode: "0o755"}`, `inventory_digest(runtime files) == targets[source.target].artifact_digest` and `inventory_digest(files minus bin/am) == manifest.artifact_digest`.
- `src/managed_source.rs` `git` and `src/client_hooks.rs` `shell_quote` made `pub` (now also in int via the leaves lane).
- `tests/t2_cli.rs`: expectations for the wired providers (pack-root refusal text, bootstrap with a missing binary refuses before any download).
- `tests/t2_delivery.rs`: recovery fixture receipt uses the runtime inventory; new `cache_qualification_binds_the_receipt_runtime_inventory_in_both_directions` (binary entry swapped, pack entry swapped, pack-only inventory).
- `tests/wiring-pending.txt`: removed the four installation-only lines; added `installation::seams::register_omp_boundary waits for omp_boundary`, `installation::seams::register_private_state waits for private_state, turn_sources`, `installation::seams::register_updating waits for updating`.
- `Cargo.toml`/`Cargo.lock`: from integration (flate2), no new crate.

## Runtime layout and binary swap

- State: `$XDG_STATE_HOME/agent-memory/install/{receipt.json,journal.json,journal-*.json,receipt-*.json,owner.json,lock}` (unchanged paths).
- Rust runtime root: `$XDG_DATA_HOME/agent-memory/runtime` = `bin/am`, `release-manifest.json`, `release-manifest.json.sshsig`, `skills/agent-memory` (release_distribution `installed` layout); receipt target kind `source`.
- `bin_dir/am`, `bin_dir/agent-memory`: symlinks to `<runtime>/bin/am`, target kind `command-link`. Development sources link to `<source>/target/release/am`.
- Python tree `$XDG_DATA_HOME/agent-memory/source` stays in place; adoption records it as kind `legacy-source` (identity unchanged, never executed or rewritten).
- Binary update atomicity: the runtime root is replaced as one directory node: the new root is staged as `<data>/agent-memory/.agent-memory-<tx>-<i>` (full copy, fsynced, identity-verified), the old root is copied to its backup, then `renamex_np(RENAME_SWAP)` / `renameat2(RENAME_EXCHANGE)` exchanges the two directory entries atomically. The command links point at the path `<runtime>/bin/am`, so new `execve`s resolve to the new inode; a running `am` keeps executing the old inode (now under the stage name, retained as rollback evidence and in `receipt.backups`), which the kernel keeps alive until that process exits. Nothing is written into an executing file.

## Public API (`agent_memory::installation`)

Constants: `RECEIPT_SCHEMA` (v3), `LEGACY_RECEIPT_SCHEMAS` (v1, v2), `JOURNAL_SCHEMA`, `OWNER_SCHEMA`, `LOCK_SCHEMA`, `PLAN_SCHEMA`, `BEGIN`, `END`, `SHIM` (include_str of the committed template), `DEFAULT_REMOTE`, `COMMAND_NAMES`, `ATOMICITY`, `MEMORY_DATA_LOCATION`, bounds `MAX_RECORD` (1 MiB), `MAX_LOCK` (65,536), `MAX_SHIM` (4,096), `MAX_GUIDANCE` (2 MiB), `MAX_MANAGED_FILE` (64 MiB), `MAX_MANAGED_ENTRIES` (40,000), `MAX_MANAGED_BYTES` (512 MiB), `MAX_REPOSITORIES` (1,024), `DEVELOPMENT_BINARY` (`target/release/am`), `RELEASE_PACK`, `RELEASE_BINARY`, `TARGET_KINDS`, `LEGACY_TARGET_KINDS`, `TARGET` (compiled triple), guidance strings `MANAGED_GUIDANCE`, `ADOPTION_GUIDANCE`, `RELEASE_GUIDANCE`, `RESTART_NOTICE`.

Types: `Error { Contract, Layout, Hook(HookCode), Release, Unregistered(&str) }`, `HookCode` (baseline `HookOwnershipError` codes/messages), `Environment` (injectable env: `process`, `from_pairs`, `home`, `directory`), `Locations { state, code, runtime }`, `InstallRequest`, `Mode`, `Configuration`, `Command`, `Action`, `Plan`, `UninstallFields`, `PrepareOptions`, `SourceInfo`, `Services` (seams + `ReleaseAccess` + `ArtifactInspection`), `ActivationHooks`, `HookVerification`, `Locked`.

Functions (baseline name → Rust): `absolute`, `locations` (+ runtime), `fingerprint`, `encoded`, `load`, `read_owner`, `locked`, `read_receipt`, `read_journal`, `activation_template` (`_activation_template`), `activation`, `presets`, `hook_paths`, `client_hook_entries` (retargeted to `<am> hook <name>`), `preflight_path`, `configuration`, `preflight_layout`, `lifecycle_paths`, `public_plan`, `source_info`, `prepare`, `verify_hook_target`, `verify_targets`, `rollback` (+ private `rollback_compatibility`, `preserve_preexchange_cache_writer`, `_rollback`, `_verify_rolled_back_targets`, `_receipt_committed`, `_require_replannable_journal`), `recover`, `install` (+ private `_install` = `install_now`/`activate`), `finish_stored_source_activation`, `failure_details`, `failure_repair`, `install_command`/`recover_command` (typed `main` for install/recover), `stage_release` (bootstrap binding), `legacy_wrapper` (`wrapper`), `legacy_source_only_wrapper`, `legacy_extension_wrapper` (Python `omp_installation.extension_wrapper`), `python_era_kind`, `receipt_is_rust_era`, `same`, `source_binary`, `pack_root`, `installation_inventory`, `omp_locations`, `omp_verify_targets`, `omp_activation_template`. Internal helpers mirror `native_rename`, `_native_rename`, `sync_dir`, `copy_node`, `sync_tree`, `write_file`, `cleanup_file`, `replace_record`, `make_state_directory`, `make_parents`, `unchanged_parent`.

`runtime_faults`: `test_root`, `operation` (generic over the caller's error type), `temporary_probe`, `mutate`, `response`, `MAX_WORLD_MARKER`. Fault points and rendezvous reuse `bank_writes::fault` (single implementation). Production guard: any of `AGENT_MEMORY_FAULT_STAGE|TRACE|ERROR_STAGE` requires the validated disposable World (`AGENT_MEMORY_TEST_ROOT` directly under canonical `/tmp`, `agent-memory-test-*`, marker token == `AGENT_MEMORY_TEST_TOKEN`), every lifecycle path inside it and the trace inside it; else typed refusal before any write — exactly the baseline rule.

## Receipt schema

Python-era v1 (read-only input; v2 = v1 + `omp_binding`): `schema`, `installation_id` (32 hex), `installed_at`, `verified: true`, `source{kind managed|release|development, path, pack, remote, ref, revision, dirty, version, artifact_digest (pack-only), compatibility[, release_manifest_sha256, signature_namespace, signature_principal]}`, `mode`, `presets`, `custom_targets`, `no_guidance`, `bin_dir`, `enabled_adapters`, `data_roots`, `memory_data_location`, `runtime_state`, `targets[{path, kind copy|link|wrapper|activation|source|hook-config|hook-link, identity, pre_change_identity, artifact_digest?, block_hash?, originally_absent?, hook_entry?, hook_original_shape?, stop_entry?, stop_original_shape?}]`, `artifact_files` (pack inventory), `backups`, `created_directories`, `rollback_available`, `atomicity`[, `omp_binding`][, `uninstalled`, `uninstalled_at`, `preserved`, `reinstall`].

Rust-era v3 (written only inside the journal transaction): same fields, plus/changed:
- `schema: agent-memory/install-receipt-v3` (Python's tree refuses it as unknown).
- `source.target`: compiled triple; `source.artifact_digest` = `artifact::inspect_runtime(pack, binary).artifact_digest` (one artifact digest); release sources carry the release fields; development sources record `target` too.
- `source.path/pack`: the owned runtime root (release) or the development source.
- `artifact_files` = `inspect_runtime(...).files` (includes `bin/am`).
- target kinds add `command-link` (bin_dir names → runtime binary) and `legacy-source` (inert Python tree, carries `legacy_source` = the Python source record). `wrapper` remains only for the OMP extension file.
- `omp_binding` re-issued (rollout attestation renewed).
Journal records `adoption: {from_schema, prior_receipt_sha256, rollback_to_python_after_commit: false}` and the result returns it plus `restart_notice`.

Journal (`agent-memory/install-journal-v1`, unchanged format): `schema, operation, transaction, status (staging|activating|complete|rolled-back|rollback-conflict), actions[{path, kind, before, after, stage, backup}], receipt, receipt_identity, created_directories, previous_compatibility, previous_artifact_digest, private_state_evidence, rollback_repositories[, activated, opaque_cache_recovery, preserved_opaque_source, conflicts, adoption]`. Step kinds accepted for finish/rollback: `copy, link, wrapper, activation, source, hook-config, hook-link, command-link, legacy-source`; any other kind refuses before any write ("Lifecycle journal step kind is unknown; preserve …"). Logical write kinds traced through `runtime_faults`: `file-stage`, `stage-cleanup`, `record-publication`, `atomic-node-switch`, `payload-snapshot`, `state-parent-directory`, `target-parent-directory`, `link-stage`.

## Python-era adoption rows → tests

Fixtures are the genuine v0.0.14 captures reduced by `derive.sh`; tests relocate `@WORLD@`, materialize owned trees as opaque bytes and recompute recorded identities (and the committed journal's `receipt_identity`). `captured_wrappers_and_hook_commands_are_exactly_the_recognised_python_era_bytes` proves the recognisers reproduce the captured wrapper bytes and hook command strings for all four scenarios.

| Row | Behaviour | Tests |
|---|---|---|
| Receipt v1/v2 | decoded; every owned target verified before replacement; v3 written only in the journal | `adoption_takes_over_every_python_era_target_and_preserves_history`, `adoption_succeeds_for_every_captured_scenario`, refusals `adoption_refuses_an_unknown_receipt_schema`, `adoption_refuses_a_drifted_owned_target_naming_it` |
| Journal history `journal-*.json` | byte-preserved | `adoption_takes_over_every_python_era_target_and_preserves_history` |
| Journal in flight | finished/rolled back from recorded steps when kinds are known; else refused, evidence kept | `adoption_rolls_back_an_in_flight_python_journal_before_starting`, `adoption_refuses_an_in_flight_journal_with_unknown_step_kinds` |
| `owner.json`/`lock` | same paths, same flock, same inode/bytes; live owner = busy; dead owner proceeds | `adoption_is_busy_while_a_python_owner_holds_the_lock`, lock/owner asserts in the takeover test |
| Wrappers | replaced only when bytes equal the exact recorded wrapper; else foreign | `adoption_refuses_a_wrapper_that_is_not_the_exact_python_wrapper` |
| Client hook commands | exact prior strings only; only owned entries rewritten | takeover test, `adoption_refuses_hook_entries_that_are_not_the_exact_python_commands` |
| Activation blocks | recorded hash verified; block content replaced; modified block refused | takeover test, `adoption_refuses_a_modified_activation_block` |
| OMP binding | re-issued for the Rust digest; `--omp-peers-upgraded` required again | `adoption_reissues_the_omp_binding_only_with_renewed_rollout_attestation` (v2 receipt built from captured codex state) |
| Managed source clone | left in place, inert (`legacy-source`) | takeover test |
| Rollback | interrupted adoption rolls back to exact Python state; after commit never re-activated | `interrupted_adoption_rolls_back_before_commit_and_never_after` |
| No verified runtime | Python receipt without bootstrap/dev source refuses with re-bootstrap guidance; different managed channel refused | `python_era_receipt_without_a_verified_runtime_source_refuses_with_bootstrap_guidance` |
| Private state | not touched by installation (private_state seam) | — (row belongs to private_state) |

Adoption runs from `--bootstrap-binary` (production) or `--development-source` (tests: no network or signing in the default suite). The bootstrap path (`stage_release`: probe signed manifest, `bind_bootstrap_binary`, acquire archive, write the same bytes as `bin/am`) is implemented but exercised only up to the pre-download refusal; a real signed run is release qualification.

## Seams (one per callee; typed `Error::Unregistered(name)` when absent)

- `installation::Updating` (`banks`, `qualified_current`, `current`, `release_transition`, `compatible_surface`) — implementer: `updating`. Registration `installation::seams::register_updating` (pending).
- `installation::PrivateState` (`activation_guard`, `prepare_lifecycle`, `require_compatible`, `public_summary`) — implementer: an adapter over `private_state`. Pending: Rust `private_state::activation_guard` fixes `target_artifact` before the transition, while the baseline transition replaces the target with `prepare_lifecycle(plan)`; it also needs `runtime_root` (the candidate pack) and `turn_sources`. Private_state owner must expose target rebinding.
- `installation::OmpBoundary` (`capability`, `native_transition`, `native_activation`, `finish_native_transition`, `recover_native_binding`, `native_recovery_expected`, `complete_stored_source_activation`) — implementer: `omp_boundary`.
- `installation::ArtifactInspection` — production `RegisteredArtifacts` (artifact with registered surface/migration); tests supply a catalog.
- Release access: `ReleaseAccess` (manifest URL, assets base, download options, signers, compiled version) — production default = pinned GitHub + compiled allowed-signers.
- Provided by installation: `cli::PackRoot` (`RuntimePackRoot`: running executable must be the receipt's runtime binary; returns `source.pack`), `console::InstalledReceipt` (`InstalledReceiptProvider`), `cache_recovery::Installation` (`CacheInstallation`), `managed_source::Installation` (`ManagedSourceInfo`), `OmpActivation` installation half (`InstallationActivation<H: OmpHarness>`; harness half = T2f), `PrivateStateHooks` installation half: `read_receipt`, `verify_targets`, `installation_inventory`.
- Not wired here: `turn_check::register_scope_binding waits for scope_binding, installation` becomes "ready" now and fails `tests/wiring.rs`; installation has no provider for it (turn_check/scope_binding owner).

## Argparse (install / recover)

`install`: `--check`/`--dry-run` store_true (dest check); `--development-source`; `--remote`; `--ref`; `--mode` choices link|copy; `--presets` (help "all, none, or comma-separated agents,codex,claude,opencode,omp (omp excluded from all)"); `--omp-agent-dir`; `--omp-peers-upgraded` store_true; `--omp-layout-quiescence` append (default None; help "Refreshed layout hold attestation including the new native invocation route"); `--layout-quiescence` append default []; `--target-dir` append (default None); `--bin-dir`; `--no-guidance` store_true; new `--bootstrap-binary PATH` (install.sh). `recover`: no options. Help strings: install "Plan or install owned portable runtime targets", recover "Verify a committed transaction or roll back interrupted activation". `update`, `uninstall`, `purge-data`, `version`, `status`, `doctor` belong to their modules.

## Obligations

Fully ported (annotated in `tests/t2_installation.rs`; Main must mark them in the ledger — `obligations_ledger` reports them as "annotated id is unported" until then):
- `test_installer.py::InstallerContract.{test_unrelated_lock_and_linked_state_are_never_adopted, test_fresh_plan_link_install_repeat_and_installed_command_preserve_source_and_user_config, test_copy_replacement_backup_and_conversion_have_identical_artifact_identity, test_all_targets_preflight_before_activation_and_unrelated_types_are_preserved, test_activation_outside_edits_survive_and_inside_edits_refuse, test_owned_client_hook_lifecycle_preserves_foreign_configuration (install half), test_explicit_development_link_refresh_still_revalidates_new_content, test_recover_with_no_pending_transaction_is_read_only, test_state_inside_development_source_refuses_before_creating_a_lock, test_partial_activation_failure_rolls_back_and_retry_converges, test_bootstrap_recovers_process_exit_before_wrapper_and_lost_commit_response, test_failed_replacement_restores_existing_copies_and_the_exact_prior_receipt, test_recovery_preserves_edits_made_after_an_interruption, test_completed_recovery_checks_the_actual_receipt_and_owned_targets, test_other_clients_keep_shared_activation_without_omp_template, test_interrupted_claude_stop_install_restores_original_settings, test_bank_state_overlap_is_refused_before_any_bank_or_runtime_write}`
- `test_installer.py::MacOSCommandContract.{test_unrelated_am_file_directory_or_link_refuses_without_activating_other_targets, test_interrupted_activation_recovers_both_owned_command_names}`
- `test_lifecycle_lost_responses.py::LifecycleLostResponses.{test_install_response_is_lost_after_real_receipt_and_targets_commit, test_fault_trace_refuses_another_world_target_before_bootstrap_writes, test_fault_trace_itself_cannot_escape_into_another_world}`
- All 32 `test_install_write_{copy,link}_{before,after}[_part_N].py::…test_actual_logical_writes_in_partition_K` (every observed write of each phase interrupted with exit 86, recovered, retried to clean semantics, repeat no-op; every partition k = step mod 8 non-empty).
Partial: `test_owned_client_hook_lifecycle_preserves_foreign_configuration` uninstall half (waits for uninstallation); `test_bank_state_overlap…` uses a `MEMORY.md` marker instead of a rendered bank (bank_contract render not needed for the layout refusal).
Deferred: `OneLineBootstrapContract.*` and `MacOSCommandContract.test_aliased_tmpdir_pipe_preview_install_update_and_both_commands` (signed release fixture + updating), `MacOSCommandContract.test_uninstall_preserves_modified_am…`, `InstallerContract.test_opencode_versioned_hook_is_owned_and_removed`, `test_codex_stop_install_preserves_foreign_settings_and_uninstalls`, `test_claude_stop_ownership_preserves_foreign_hooks_and_tampering`, `test_omp_binding_installs_candidate_activation_in_copy_and_link_modes`, `test_omp_historical_*`, `test_omp_modified_or_foreign_activation_is_preserved`, `test_omp_missing_or_malformed_candidate_activation_refuses_without_fallback` (wait for uninstallation and/or omp_boundary), all `update/uninstall` partitions and other lost-response cases (updating, uninstallation, purging). Obsolete by contract change: `test_wrapper_chooses_supported_python_after_older_path_entry`, `test_actual_shipped_old_updater_then_install_migrates_without_stranding_ownership`, `test_current_and_shipped_recovery_read_each_others_wrapper_journals`, `test_shipped_uninstaller_can_remove_new_wrappers_with_aliased_xdg_and_target_paths` (shipped Python runtimes; replaced by the adoption tests), `test_managed_source_local_channel_and_external_receipt` (managed channels refuse; adopted read-only).

Unported branches (listed per the brief): `prepare`'s managed-channel clone (`git clone`) — contract change, refuses with `MANAGED_GUIDANCE`; `source_info` for `kind=release` of a *Python* release root (no `bin/am`) — adopted via legacy-source instead; `failure_details` only for download errors (as baseline). Uninstall-specific branches inside `install`/`target_record` are ported but untested until uninstallation exists.

## Declared deltas

- New receipt schema v3, target kinds `command-link`/`legacy-source`, `source.target`, runtime `artifact_files`; command wrappers become symlinks; no interpreter wrapper.
- Client hook commands are `<shell-quoted am> hook <codex-prompt|codex-stop|turn-hook|claude-stop>` (CLI lane owns the `hook` route).
- Managed `--remote/--ref`: fresh managed installs refuse with guidance; existing managed receipts are adopted read-only.
- Development sources require prebuilt `target/release/am`; the installer never runs Cargo.
- Activation/guidance files read with a 2 MiB bound (baseline unbounded); non-UTF-8 activation files refuse.
- `os.access` writability decided from owner/other mode bits of the trusted ancestor.
- Bootstrap binary missing/non-ordinary refuses before any download.
- `Python repr` of the legacy source-only launcher path is reproduced only for plain paths; others are treated as foreign.

## Commands run (macOS arm64, worktree)

`cargo fmt`; `cargo fmt --check` (clean); `cargo check --offline --all-targets` (no warnings); `cargo test --offline --test t2_installation` ×3 → 50/50 three times, then 53/53 after the last three ports and again after the final resync; `t2_cli` 15/15; `t2_delivery` 31/31; `t2_omp_install` 18/18; `t1_a2` 79/79; `wiring`: fails only on `turn_check::register_scope_binding` (see Seams); `obligations_ledger`: fails until Main marks the ids above. Panic scan `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/installation.rs src/installation/*.rs src/runtime_faults.rs src/cli/helpers/installation.rs`: no matches.
