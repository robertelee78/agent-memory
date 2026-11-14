# diaginit lane report (diagnostics + initialization)

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-diaginit`: snapshot `46a0100`. Every non-owned file that changed between `46a0100` and integration `a725b97` (security, recall_catalog, omp_stream) was refreshed from `git show a725b97:<path>`, and my appends were applied again afterwards. No Git writes, no Python executed, no sub-agents, no new crates, no network.

## Files

Created (owned):
- `src/diagnostics.rs`: baseline `diagnostics.py` (`am version|status|doctor`), plus `diagnostics::Runtime`, the production implementer of `doctor_fix::Diagnostics`.
- `src/initialization.rs`: baseline `initialization.py` (`am init inspect|read|preview|apply`), plus `initialization::Validator`, the `bank_transactions::TransactionValidator` for `initialize`.
- `src/cli/helpers/diagnostics.rs`: natives `VERSION_NATIVE`, `STATUS_NATIVE`, `DOCTOR_NATIVE`.
- `src/cli/helpers/initialization.rs`: helper `initialize.py` → `am init`.
- `tests/t2_diagnostics.rs`: 24 run, 6 ignored.
- `tests/t2_initialization.rs`: 10 run, 1 ignored.

Shared files (append-only):
- `src/lib.rs`: `pub mod diagnostics;` and `pub mod initialization;`, alphabetical.
- `src/cli/helpers/mod.rs`: `pub mod diagnostics;` and `pub mod initialization;`.
- `src/cli/table.rs`:
  - `route("init", …)` after `identity`. Baseline route order is developer, project, identity, init.
  - `VERSION_NATIVE`, `STATUS_NATIVE`, `DOCTOR_NATIVE` after `RECOVER_NATIVE`. Baseline order is install, update, uninstall, purge-data, recover, version, status, doctor.
- `src/cli/mod.rs` `register_providers` (appended):
  - `doctor_fix::register_diagnostics(Runtime::process())`;
  - `bank_transactions::register_transaction_validator("initialize", initialization::Validator)`.
- `tests/wiring-pending.txt`:
  - Removed `doctor_fix::register_diagnostics …`.
  - Removed `bank_transactions::register_transaction_validator waits for admission, consolidation, governance, initialization, memory_refresh`. The registration point is now called, and `tests/wiring.rs` forbids a point that is both wired and pending.
    - The other operation kinds keep refusing with `Transaction validator is not registered for operation kind: <op>` until their lanes append their own call.
    - **Main: the reminder for admission/consolidation/governance/memory_refresh is gone with this line.**
  - Added `diagnostics::register_projections waits for projections`.
- `src/turn_check.rs` trait `MemoryRefresh` (implements the frozen T2e→T2c interface from the design):
  - appended required method `local_prerequisites(&self, repo) -> Result<Result<Value, CurrentFailure>>`;
  - appended `pub struct CurrentFailure {code, message, repair, path}` and `pub(crate) fn memory_refresh_provider()` after the last item.
  - Implementer: `memory_refresh`.
- `tests/t2_lifecycle.rs`:
  - `healthy_installation_needs_no_doctor_repair` is un-ignored. It runs `am doctor --fix` through `diagnostics::command` (fix=true) in the existing child harness, via a new child arm `doctor-fix` that registers `diagnostics::Runtime::new(env, running_services)` and the production `security::Security` scan.
  - It also writes a foreign `user-setting` into `hooks.json`, as the baseline does, and asserts exit 0.
- `tests/obligations/python-baseline-v1.json`: marked with `ledger-mark.sh` from this worktree (19 ported, 36 partial). `test_doctor_fix … test_healthy_unrelated_hook_configuration_needs_no_repair` was moved from `partial` to `ported` with `am-dev obligations mark`.

Foundation changes: none.

`initialization` calls `security::ensure_safe` directly. An earlier append of `ensure_safe` to `bank_writes::Security` was withdrawn, so `src/bank_writes.rs` is unchanged from base.

## Public API

`diagnostics`:
- Constants: `SCHEMA`, `RECOVER`, `MAX_READ` (1 MiB), `MAX_ROUTER` (16 KiB), `MAX_LOCK` (64 KiB), `MAX_PURGES` (1024).
- Types: `Failure {Fixed, Error}`, `Outcome<T>`, `Report {new, add, value, finish}`, `Request {operation, repo, projection, fix}`, `Runtime {new, process}`.
- Functions: `attempt`, `read`, `record`, `executing_package`, `source`, `channel`, `security_summary`, `distribution_notice`, `receipt_view`, `hook_migration`, `tracked_bank_roots`, `lightweight_bank`, `identities`, `permission_check`, `runtime_doctor`, `bank_doctor`, `setup_assessment`, `shallow_purge_state`, `run`, `command` (baseline `execute`).
- Seam: `Projections {max_bytes, value}`, `register_projections`; `projections_provider` and `which` are pub(crate).

`initialization`:
- Constants: `SCHEMA`, `RESULT_SCHEMA`, `AWA_SCHEMA`, `EXCLUDED`, `CODE`, `MAX_SOURCES`, `MAX_FILE`, `MAX_DISCOVERED`, `MAX_AWA` (1 MiB), `MAX_DEPTH`.
- Functions: `result`, `root_at`, `source_kind`, `read_source`, `local_sources`, `awa_evidence`, `inspect`, `link`, `review_value`, `implementation_observation`, `claim_lines`, `orientation`, `new_claims`, `preview`, `check_sources`, `transaction_evidence`, `validate_transaction`, `apply`, `command_result` (baseline `main` output rule).
- Type: `Validator`.

## Registration traits

- Implemented and wired:
  - `doctor_fix::Diagnostics` by `diagnostics::Runtime`;
  - `TransactionValidator("initialize")` by `initialization::Validator`. Its `validate_transaction` is the baseline `validate_transaction`; its `check_sources` rechecks the recorded inventory.
- Introduced: `diagnostics::Projections`, implementer `projections`, pending line added. While unregistered, `--projection` gives a `bank.projection` failure: "Projections (reference projection reader) is not registered for this runtime; this component cannot be inspected."
- Extended: `turn_check::MemoryRefresh::local_prerequisites` + `CurrentFailure`, implementer `memory_refresh`. While unregistered, a located bank gets a `bank.refresh_prerequisites` failure naming the provider.

## Production status at a725b97

- `am version`, `am status`: complete.
- `am doctor` with an installed runtime and no bank: healthy, exit 0.
- `am doctor` on a located bank:
  - `bank.refresh_prerequisites` fails until `memory_refresh` exists.
  - `bank.inspection` fails until `recovery_frontier` exists, because `bank_observation::observe` requires it.
  - `identity.authority` therefore is not reached.
- `am doctor --fix`: wired end to end.
- `am init inspect|read|preview`: complete and tested.
  - `inspect` of an existing bank reports `needs-inspection` until `recovery_frontier` exists.
- `am init apply`: implemented. Its write path (`bank_transactions::apply`) refuses with `recovery_frontier provider is not registered`. Identity `bind` (scope_binding) has the same dependency.

## CLI declarations

- `version`, help "Read-only runtime and bank diagnostics": `--json`.
- `status`: `--json`; `--repo` (default `.`).
- `doctor`: `--json`; `--repo` (default `.`); `--projection`; `--fix`, help "Repair safely identified owned runtime faults and recheck".
- `init`, routed to `initialize.py`; its description is the module docstring:
  - `inspect --repo . --awa-evidence --source (append)`;
  - `read --repo . --path (required)`;
  - `preview --repo . --review (required)`;
  - `apply --proposal (required)`.

Output rules:
- Diagnostics prints INDENT2_ASCII and exits 0 iff `ok`. A refusal outside any component becomes the baseline `diagnostic.inputs` report, or the bank-capacity report.
- Init success prints INDENT2_UTF8. A refusal prints the compact `DEFAULT` envelope with `error`, `verified:false`, `inspection_required:true` and capacity details, exit 1.

## Obligations

### Ported (20)

`test_runtime_diagnostics`:
- `test_no_install_bank_or_identity_is_required_and_nothing_is_bootstrapped`
- `test_status_is_shallow_but_doctor_checks_real_bank_content_and_permissions`
- `test_unknown_bank_schema_and_redirected_router_are_not_accepted`
- `test_owner_record_diagnostics_agree_with_actual_recovery_and_preserve_bad_metadata`
- `test_live_runtime_lock_is_not_stolen_or_treated_as_an_orphan`
- `test_changed_link_and_activation_block_are_preserved`
- `test_missing_source_and_committed_receipt_corruption_are_distinct_failures`
- `test_credential_urls_and_corrupt_records_do_not_escape_diagnostics`
- `test_prompt_only_legacy_and_invalid_present_stop_receipts`
- `test_opencode_plugin_link_remains_exact`
- `test_hook_verification_refuses_an_edit_during_readback` (real fault-pause rendezvous)

`test_hook_migration_diagnostics`:
- `test_missing_and_intentionally_disabled_hook_targets`

`test_initialization`:
- `test_legacy_invalid_memory_and_visible_markers_are_preserved_while_local_sources_remain_inspectable`
- `test_discovery_and_reads_are_bounded_readonly_and_awa_gap_does_not_hide_local_evidence`
- `test_explicit_source_selection_respects_ignore_and_unknown_conditions_do_not_become_guidance`
- `test_awa_skill_handoff_keeps_work_planned_and_wrong_project_or_unavailable_evidence_honest`
- `test_local_source_hrefs_resolve_exact_punctuation_unicode_and_percent_bytes`
- `test_actual_peer_intent_conflict_prompts_detecting_developer_and_never_chooses_landing_order`
- `test_implementation_conditions_match_known_branch_and_exact_revision`

`test_doctor_fix`:
- `test_healthy_unrelated_hook_configuration_needs_no_repair`

### Partial: the test runs, and the listed part is missing

`test_runtime_diagnostics`:
- `test_healthy_copy_and_link_installs_and_allowed_outside_activation_edits`: no bank, so `counts_by_lifecycle` and `index_freshness` are not asserted. They need `recovery_frontier` and `memory_refresh`.
- `test_changed_wrapper_and_copy_are_reported_from_an_outside_helper`: Rust installs command links, not wrapper files. A replaced `agent-memory` link produces a `runtime.target.command-link` failure. The `0o644` wrapper-mode detail is not tested; in code it applies to the OMP extension wrapper.
- `test_installed_version_binds_actual_artifact_revision_mode_and_channel`: run with a development source in Git (revision, clean, digest, mode, schema). The managed `ref: main` check does not apply, since managed channels are not installable.
- `test_dirty_managed_source_and_changed_channel_are_not_healthy`: a dirty development source gives a `runtime.source` warning. The managed remote change does not apply.
- `test_pending_runtime_and_bank_transactions_are_observed_without_recovery`: the components are asserted. The baseline exit 0 is 1 here, because of refresh prerequisites.
- `test_shared_owned_hook_health_and_negative_controls` and `test_insufficient_hook_receipt_is_not_adopted_from_matching_bytes`: every doctor case is ported. The `uninstall --check` preserved-list assertions are not.

`test_setup_diagnostics`:
- `test_new_repository_and_precreated_identity_need_setup_without_invented_mismatch`: the JSON status/doctor setup states are asserted. The console human lines (`setup …`, `next …`) are not; console render belongs to the CLI lane. The `am --json doctor` / human run in `am_diagnostics_run_natively_against_a_disposable_bank` covers rendering of the `setup` line.

### Deferred: ignored tests with obligation lines, marked partial

- `needs projections registered`: `test_selected_projection_current_and_stale_do_not_create_audit_or_cache`.
- `needs recovery_frontier registered`:
  - `test_resolved_identity_and_custom_bank_follow_the_selected_worktree` and `test_bank_binding_required_matching_and_actual_mismatch_are_distinct`. Both test bodies are written and run up to `identity bind`.
  - `test_doctor_does_not_resume_a_separately_confirmed_purge_or_initialize_replacement` (purge-data).
  - All `apply`-based `test_initialization` tests (12) and the 3 `test_initialization_routes` tests.
- `needs omp_boundary registered`: the 4 `test_native_*` tests.
- `needs memory_refresh registered`: the other 4 `test_setup_diagnostics` tests.

### Not annotated (other lanes or modules)

- The remaining `test_doctor_fix.*`: bank-transaction and update-recovery repairs.
- `test_checkout_repair.*`: needs the admission fixture.
- `test_read_only_audits.*`, `test_installed_bank_initialization`, `test_independent_local_bank`, `test_offline_skill`: need checkpoint, retrieval and admission.

## Declared deltas

Diagnostics:
- **Executing package** (baseline `artifact.PACK`): the receipt-owned runtime of the running executable, otherwise a development build at `<source>/target/release/am` beside `<source>/skills/agent-memory`. Anything else is a fixed `runtime.executing_artifact` failure. The digest binds the binary; a legacy managed checkout is inspected pack-only.
- **No `prerequisite.python`.** `filesystem.native_api` is a compile-time property (`renamex_np` / `renameat2`).
- **Python-era (v1/v2) active receipt:** it is never security-verified. `runtime_doctor` reports a `runtime.ownership` failure with `ADOPTION_GUIDANCE` instead of auditing Python wrappers.
- **Rust-era target kinds:**
  - `command-link` must be `bin_dir/{am,agent-memory}`.
  - `wrapper` means only the OMP extension; its bytes must equal `omp_installation::extension_wrapper` of the runtime binary.
  - `legacy-source` is checked through `installation::verify_targets`.
- **Hook migration** compares against `client_hook_entries(name, recorded runtime binary)`.
- **Exception text never leaks.** A non-fixed component failure reads "Cannot verify this component from its current local evidence.". An unregistered provider reads "<name> is not registered for this runtime; this component cannot be inspected.".
- **`os.access`** is decided from owner/other mode bits; root needs any execute bit.
- **`shutil.which`** checks for an executable regular file on `PATH`.
- **Router reads** are UTF-8-strict.

Initialization:
- **Discovery:**
  - non-regular files (FIFOs, sockets) are skipped like links;
  - directories deeper than 256 add the gap "Local source discovery hit its directory depth bound; inspect deeper relevant sources explicitly";
  - a non-UTF-8 path gets the existing "cannot be represented safely" gap;
  - listings are bounded at 1,000,000 entries.
- **Bounds:** AWA evidence is read with a 1 MiB bound. `read` of non-UTF-8 content refuses with the shared non-UTF-8 message.
- **`apply`** creates missing bank directories with mode 0755 via pinned `mkdirat`, and fsyncs the parent.
- **Envelope `error` texts** for OS errors are the foundation's messages.

## Test fixtures

- Seam fixtures as in `t2_installation`: `FixtureUpdating`, `FixturePrivateState`, `FixtureArtifacts` with the catalog.
- Diagnostics and init run in a child copy of the test binary whose only environment is the World's. Identity and registry roots read the process environment, so this keeps the user's HOME out.
- The child calls the real `cli::register_providers()`, including production security.
- `t2_initialization` children also register `SourcePack`, a `cli::PackRoot` returning this worktree's `skills/agent-memory`. That is what baseline `artifact.PACK` is when helpers run from the source checkout. The production provider requires an installed receipt, which a test binary does not have.

## Process notes

- One edit early on (`fn which` → `pub(crate)`) was made with `sed -i` instead of the edit tool.
- My first `ledger-mark.sh` runs went into the integration ledger, because of the script defect; Main reverted them. The marks were then made from this worktree.

## Commands (macOS arm64: aarch64-apple-darwin, Darwin 27.3.0, rustc 1.99.1)

- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: 0 warnings.
- `cargo test --offline --features dev-tools --test t2_diagnostics --test t2_initialization`, three consecutive runs on the final tree: 24 passed / 6 ignored and 10 passed / 1 ignored each time.
- `cargo test --offline --features dev-tools --test t2_lifecycle healthy_installation`: 1 passed.
- Whole suite `cargo test --offline --no-fail-fast --features dev-tools` on the refreshed tree (a725b97 non-owned files): exit 0; every test binary green, 0 failures. That includes `obligations_ledger` 5/5 and `wiring` 2 passed / 1 ignored (the cutover gate).
- Base: I finished on `a725b97`. `d1b2766` (omp_boundary, layout core, the private-state seam, RecoveryFrontier trait appends) was not pulled in, because `recovery_frontier` itself is still unported and is what my deferred tests wait for.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/diagnostics.rs src/initialization.rs src/cli/helpers/{diagnostics,initialization}.rs`: no matches.
- Not run: Linux musl (coordinator).
