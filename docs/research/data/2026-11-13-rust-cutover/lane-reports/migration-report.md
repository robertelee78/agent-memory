# migration lane report — `migration` (T2b)

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-migration`: started from integration `46a0100`, then refreshed (Main-approved) to `a725b97` (security on top of recall_catalog/omp_stream): every non-owned path in `git diff --name-only 46a0100 a725b97` (40 paths) was copied with `git show a725b97:<path>`, then my appends were re-applied. No Git writes.

## Files

Created (owned):
- `src/migration.rs` — constants (`SCHEMA`, `MAX_FILES`, `MAX_FILE`, `MAX_BYTES`, `MAX_PLAN`, `MAX_RECORD`, `MAX_ROUTER`, `FIELDS`, `RETAINED`, `PACK_ENTRY`, `BINARY_ENTRY`, `DEVELOPMENT_BINARY`), `Runtime` (baseline `PACK`), `executable_root`, `result`, `router_metadata`, `locate`, `bounded_files`, `read_regular`, `snapshot`, `catalog`, `validate_codec`, `transform`, `context`, `hashes`, `private_directory`, `locations`/`locations_at`, `read_record`, `keep_record`, `keep_blob`, `retain_runtime`, `retained_helper`, `audit`, `LocationAudit` (implements `security::MigrationAudit`).
- `src/migration/plan.rs` — `proposal_value`, `check_code`, `check_source`, `preview`, `marker_value`, `owned_marker`, `stage`, `read_intent`, `read_journal`, `receipt_value`, `partial`, `historical`, `converge`, `apply`, `parent_records`, `restoration_payload`, `restore_preview`.
- `src/migration/recovery.rs` — `RECOVERY_FIELDS`, `recovery_view`, `recover` (+ private `cancel`: the preparation-cancelled rollback receipt), `recover_lock`.
- `src/migration/command.rs` — `Request`, `input_value`, `command` (baseline `main` minus argparse: success = indent-2 UTF-8 JSON exit 0; failure = compact inspection envelope exit 1).
- `src/cli/helpers/migration.rs` — `am migrate` parser (preview/apply/recover/recover-lock/restore, exact baseline options and `--mode` choices), `HELPER`, `current_runtime`, `run_with`, `dispatch_with`.
- `tests/t2_migration.rs` — 13 tests (11 annotated obligations, 1 route test, 1 re-exec worker).

Shared files (append-only):
- `src/lib.rs`: `pub mod migration;` (alphabetical).
- `src/cli/helpers/mod.rs`: `pub mod migration;`.
- `src/cli/table.rs`: `route("migrate", &helpers::migration::HELPER)` after `writes` (baseline MEMORY_ROUTES order: …, writes, transaction, migrate; `transaction` is not registered yet).
- `src/cli/mod.rs` `register_providers`: appended `security::register_migration_audit(Some(Arc::new(migration::LocationAudit::executable())))`.
- `tests/wiring-pending.txt`: deleted `security::register_migration_audit waits for migration`.
- `tests/obligations/python-baseline-v1.json`: via `ledger-mark.sh`.

Python, schemas, templates, guides untouched.

## Baseline coverage (every function of `migration.py`)

`result router_metadata locate bounded_files read_regular snapshot catalog invoke validate_codec transform context hashes private_directory locations read_record keep_record keep_blob retain_runtime proposal_value check_code check_source preview marker_value owned_marker stage read_intent read_journal receipt_value partial current_result historical converge apply parent_records restoration_payload restore_preview optional_record_sha recovery_view recover recover_lock input_value main` — all ported. `invoke` is the compiled-codec lookup (`migration_contract::Codecs`); `router_metadata` reuses `updating::router_metadata` (already the baseline port) instead of a second copy.

## Runtime model (design rule F)

- `Runtime { root, pack = root/skills/agent-memory, binary, codecs, surface }`. Production `am migrate` self-locates from `current_exe`: `<root>/bin/am` (release/retained) or `<root>/target/release/am` (development source); anything else refuses "This executable is not inside a runtime with a portable pack; run the installed or retained am".
- `catalog`/`check_code` use `artifact::inspect_runtime_with(pack, binary, surface, catalog)`: the plan's `artifact_digest` is the runtime digest (pack + `bin/am`), so a plan never resumes under a different binary; `catalog_sha256` is the catalog-v2 file digest.
- `retain_runtime` copies every inventory entry: pack files to `trusted-package/skills/agent-memory/<path>`, the binary to `trusted-package/bin/am`, each re-hashed against the measured identity, then proves `trusted-package` inspects to the same artifact digest. `retained_helper` reports `…/trusted-package/bin/am` (an executable runtime root, runnable without any global install).
- `locations` protects the executing runtime root, Python-era `XDG_DATA_HOME/agent-memory/source`, the Rust runtime `XDG_DATA_HOME/agent-memory/runtime`, and receipt targets of kind source/copy/link.

## Seams

- Implemented: `security::MigrationAudit` → `migration::LocationAudit` (`locate(root) == (root, bank)` and `locations_at(runtime_root, …)`); production registration uses the running executable's runtime root, tests register `LocationAudit::at(fixture_root)`.
- Called directly (ported): `security::audit(…, ["external"], [], [], action, Some(bank))`, `security::scan(message, security::LIMIT)`, `scope_binding::require` (with the migration router reader), `turn_sources::canonical_writer_barrier(root, None, runtime_root)`, `local_registry::{exclusive, recover, now}`, `bank_transactions::{MARKER, create_marker, check_marker, replace_file, flush_directory}`, `bank_writes::{state_paths, fault}`, `installation::{locations, read_receipt, same}`, `artifact::inspect_runtime_with`, `migration_contract::{inspect, route, Codecs}`, `repository_config::{settings_with, validate_root}`, `bank_contract::{bank_root, path_value, file_topology, portable_aliases, runtime_path}`, `bank_capacity::details`.
- No new registration traits.

## CLI

`am migrate {preview --repo . --to SCHEMA | apply --proposal FILE | recover [--repo .] [--mode finish|rollback] [--operation ID] [--proposal FILE] | recover-lock [--repo .] [--proposal FILE] | restore [--repo .] [--operation ID] [--proposal FILE]}`; description = module docstring. Console `HUMAN` rendering of `migrate` already existed in `console.rs`.

## Declared deltas

1. `retained_helper` is `<op>/trusted-package/bin/am` (was `…/scripts/migrate_bank.py`); the retained package is a runtime root (pack + binary).
2. `error_type` is always `"ContractError"` (Python class names of OSError/UnicodeError/… are interpreter-dependent; every failure is a typed refusal). `SyntaxError` withholding has no counterpart; a scan failure withholds the message like a scan reason.
3. Hash/mode maps in proposals and records are path-sorted (baseline `os.walk` order was filesystem-dependent); on-disk records were already `sort_keys` encoded, so record bytes are unchanged.
4. Directory listings during `snapshot` are bounded by `MAX_FILES` entries per directory ("Migration source inventory exceeds its complete bound").
5. Non-UTF-8 bank paths/bodies refuse with the shared non-UTF-8 refusal (baseline raised UnicodeDecodeError).
6. A fault/unknown read error maps to the baseline refusal texts: changed during read → "Migration source changed during its bounded read"; non-regular/linked/oversized → "Migration reads bounded ordinary single-link files only"; I/O errors keep their OS text.
7. `keep_blob` publishes through a pinned parent descriptor (`create_exclusive` 0600/`mode` + fsync + `rename_at(NoReplace)` + directory flush) instead of `NamedTemporaryFile` + path rename; same no-replace semantics.

## Obligations

Annotated (11, `tests/contracts/test_explicit_migration.py::ExplicitMigrationContract.*`): production_native_noop…, complete_multistep…, completed_restore…retained_helper…, stale_proposal…, interrupted_conversion…, receipt_backup_pin…, interrupted_unknown_new_edits…, restoration_interruption…, changed_state_root…, migration_preserves_uncommitted…, live_lock…. All currently `#[ignore = "needs recovery_frontier registered"]`: the fixture identity binding (`am identity preview/apply`) and bank validation refuse "recovery_frontier provider is not registered". Ledger status: partial (to be re-marked ported after the recovery_frontier refresh and un-ignore).

Partial by construction (will stay partial):
- `test_completed_restore…retained_helper_works_without_global_install`: the restoration runs as the retained runtime root after the original runtime binary is removed, but test-only codecs cannot be compiled into a shipped binary, so the retained `bin/am` (a stand-in) is not executed as a process.
- `test_migration_preserves_uncommitted…rejects_generic_recovery`: the generic recovery is `bank_transactions::recover` in the worker; `am transaction` has no CLI route yet.

Deferred (not annotated):
- `test_explicit_migration.py::…test_fresh_and_existing_install_check_and_apply_refuse_unsupported_bank_without_activation` — installer/updating bank compatibility gate (installation, updating).
- `test_migration_runtime_rollback.py::…` — partial runtime update + installer recover with a Python-era v1 catalog (updating/installation; Python-era catalog refusal by design).
- `test_update_mutable_observations.py::…` — updating's router observation race (patches `migration.read_regular`); belongs to updating.
- `test_hook_migration_diagnostics.py::…` — diagnostics.
- `test_migration_catalog.py::…` (2) — already partial (migration_contract lane).

## Test design

Every migration command runs in a child copy of the test binary (`migration_worker`, selected by `AM_MIGRATION_*` variables added to the World env): it calls `cli::register_providers()`, registers `LocationAudit::at(fixture_root)`, and runs `cli::helpers::migration::dispatch_with` with a fixture runtime (`trusted-fixture-source`: minimal valid portable pack + catalog v2 + stand-in `bin/am` 0755) and either the production `Compiled` codecs (production test) or the test-only fixture codecs (port of `migration_fixtures.CODE`: `legacy`, `middle`, `first`, `second`). Faults (`exit` → 86, `pause` rendezvous) act on that child only. Banks are rendered with `am bank render`, identities bound with the shared `identity_fixtures::bind`.

Fixture banks are rendered with `am bank render` only. The baseline's follow-up `bank validate` is omitted because `am bank validate` currently refuses "recovery_frontier provider is not registered". Re-add it with the un-ignore.

## Foundation changes

None. During development I briefly appended a `bank_writes::Security::migration_audit` seam. Main rejected it in favour of calling `security::audit` directly, and I removed it. `src/bank_writes.rs` is byte-identical to the base.

## Verification

Platform: aarch64-apple-darwin, Darwin 27.3.0 arm64, rustc 1.99.1 (58cf8f9c3 2026-10-13). Linux targets were compile-checked only.

Lane tree, after the refresh to `a725b97`:
- `cargo fmt --check`: clean. My files were formatted with `rustfmt --edition 2024`.
- `cargo check --offline --all-targets --features dev-tools`: 0 warnings, 0 errors.
- `cargo check --offline --features dev-tools --lib --target x86_64-unknown-linux-musl`, and the same for `aarch64-unknown-linux-musl`: clean.
- `cargo test --offline --features dev-tools --test t2_migration`, 3 runs: 2 passed, 0 failed, 11 ignored each time.
- `--test t2_migration -- --ignored production_native`: fails at fixture `bind`, as expected. `am identity preview` returns the scope-result error "recovery_frontier provider is not registered", which confirms the ignore reason.
- `--test wiring --test obligations_ledger`: 2 passed + 1 ignored, and 5 passed.
- Whole suite `cargo test --offline --no-fail-fast --features dev-tools`: exit 0. Summing all `test result` lines in `/tmp/am-migration-full.log` gives 60 lines, 898 passed, 0 failed, 60 ignored. Nested re-exec child summaries are included in those sums.
- `~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh - tests/t2_migration.rs` (run from the lane worktree) reported `marked ported=0 partial=11`.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/migration.rs src/migration/*.rs src/cli/helpers/migration.rs` matched only a doc comment (`src/migration.rs:823`).

Scratch evidence only, not lane validation (as Main instructed):
- The scratch copy was `/tmp/am-migration-scratch` on base `46a0100`, plus int's working-tree `security.rs`/Cargo files, plus a throwaway `RecoveryFrontier` stub (no hold; `observe` → `hold_sha256: null`).
- In it, `t2_migration` passed 13/13 with nothing ignored, three runs of about 125 s each.
- The copy and the stub are deleted. They never touched the lane tree.

## Hand-over / remaining

- When recovery_frontier lands (Main sends the hash), refresh again, then remove the 11 `#[ignore = "needs recovery_frontier registered"]` lines.
- Re-add `am bank validate` to the `bank()` fixture.
- Run `t2_migration` 3×.
- Mark the 9 non-partial-by-construction obligations `ported`. `ledger-mark.sh` only touches `unported` ids, so this needs `./target/debug/am-dev obligations mark --status ported --ids-file <ids>` directly.

## Process notes (honesty)

- While building the scratch copy, one shell command contained an empty `python3 - <<'EOF'` heredoc (no code, `|| true`). Nothing was inspected or executed by it.
- Scratch-copy edits were made with `sed`/`perl`. No `sed`/`perl` edits were made in the lane tree.
