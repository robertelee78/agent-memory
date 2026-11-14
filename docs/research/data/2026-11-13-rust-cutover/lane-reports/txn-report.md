# bank_transactions handover

## Result

All baseline bank_transactions.py functions/branches are implemented as typed Rust APIs, including proposal validation, preview, full governed apply, trusted mechanical apply, recovery preview/apply, finish/rollback convergence, historical receipts, source attribution, journal/marker cleanup, and CLI result formatting. Argparse itself is deliberately replaced by typed functions. No Python ran, no Git writes occurred, no crates were added, and no subprocess is introduced by this module.

`bank_transactions ready` was sent after the module compiled and the first 20 independent tests passed. Final target: **25 passed, 2 explicitly ignored**, three consecutive successful runs. Full domain-integrated writer tests remain dependent on later providers; no fake domain providers were installed.

## Exact files

New owned files:
- `src/bank_transactions.rs`
- `tests/t1_bank_transactions.rs`

Modified shared files (take these union versions, not just the original observer-lane versions):
- `src/lib.rs`: alphabetical registrations for bank_transactions and imported bank_observation/bank_state/tombstones.
- `src/bank_contract.rs`: required `BankObservation::scan_sources`, public forwarding function, `bank_observation_provider`, `recovery_coverage_provider`; forwarding uses these accessors.
- `src/bank_writes.rs`: required trait additions below, six crate-visible provider accessors, plus observer lane's RecoveryFrontier methods.
- `src/repository_config.rs`: observer lane's required `SecurityScan::scan` and `security_scan_provider`; formatted the added accessor.
- `src/fs/descriptor.rs`: T0 `create_directory` and `remove_directory_at`, described below.

Refreshed read-only copies from Main's requested hardened snapshot (`/opt/agent-memory-worktrees/rust-v015-int`, 7622e91): `src/artifact.rs`, `src/pack_validation.rs`, `src/bank_layout.rs`, `tests/t1_bank.rs`, plus bank_contract/bank_writes/descriptor before the additions above. Baseline `t1_bank` confirmed 30/30 before extending shared APIs.

Imported read-only from the green observation lane: `src/bank_observation.rs`, `src/bank_state.rs`, `src/tombstones.rs`. No implementation edits were made to these three files.

No Cargo.toml/error.rs edits. This report is the only Markdown file created.

## Public API

Constants: MARKER, SCHEMA, RECOVERY_SCHEMA, FIELDS, RECOVERY_FIELDS, RECEIPT_FIELDS, MAX_MARKER_BYTES, MAX_JOURNAL_BYTES, MAX_RECEIPT_BYTES.

Typed public operations:
- `preview(repo, files, operation)`, `preview_with_sources(..., Sources)`.
- `apply(prepared)` and `recover(repo, mode: Option<&str>, prepared: Option<&Value>)`.
- Trusted domain-call surfaces replacing baseline `_apply` / `_recover`: `apply_with_identity`, `apply_with_identity_and_environment`, `recover_with_identity`, `recover_with_identity_and_environment`. Explicit `StateEnvironment` and `Sources` variants keep filesystem tests away from user state.
- `converge(root, bank, proposal, base_files, journal, raw, receipt_path, mode, observed, owner, recovery, Sources)`.
- `replace_file(bank, path, body: Option<&str>, expected, marker, participants: &BTreeSet<String>, mode_bits: Option<u32>, before_replace: Option<&dyn Fn() -> Result<()>>)`. Exact existing/default/explicit file modes, whole-read-set and physical-layout rechecks, exclusive same-directory temporary creation, temporary fsync, pinned-parent `fs::rename_at(..., RenameMode::Replace)` or unlink, then parent fsync.
- Baseline helpers: result, file_hashes, files_at, proposal_value, check_layout, preflight, locations/locations_with_environment, changed, marker_bytes, flush_directory, create_marker, check_marker, check_observed, cleanup, receipt_value, admission_recovery, journal_value, allowed_partial, invalidate_optional_indexes, validate_outcome, source_classes.
- `command_result(operation, Result<Value>)` returns existing CommandResult with baseline JSON stdout and exit semantics.

Durable marker/journal/receipt bytes use developer_identity::encoded (sorted keys, indent=2, UTF-8, finite numbers, trailing newline). Journals/markers/receipts remain private 0600. Lock is exactly `<bank>/.agent-memory.lock`, storage::exclusive's native flock, operation names `bank-<operation>` / `bank-recover`. Marker creation is O_EXCL; no unknown marker is replaced. Router changes run last during convergence. Verified receipt precedes marker deletion and journal cleanup; historical outcomes cannot switch direction or replay over newer edits.

## New registries / wiring

`TransactionValidator` (implemented later by initialization, admission, consolidation, governance, memory_refresh):
- `validate_transaction(&self, proposal: &Value, owned_lock: Option<&[u8]>) -> Result<()>`
- `check_sources(&self, proposal: &Value, owner: &[u8], marker: &[u8]) -> Result<()>`
- Register each governed operation with `register_transaction_validator(operation, Some(Arc::new(Provider)))?`; operation-keyed shared accessor is `transaction_validator(operation)` for recovery_transactions. `validate_operation` preserves the baseline precondition: a validator is consulted only for governed evidence or initialize. Raw mechanical replace/compact/correct without evidence does not gain a new gate. Missing required registration is the typed refusal `Transaction validator is not registered for operation kind: <op>`. Main explicitly confirmed this interpretation.
- Governance registers correct/supersede/revoke/forget; other implementers register initialize/capture/compact/refresh respectively. Checkpoint uses the existing bank_writes::Checkpoint trait, not this registry.

`IndexInvalidation` (recall_catalog): `invalidate_after_change(&self, repo: &Path, hashes: &Value) -> Result<Value>`; register with `register_index_invalidation(Some(Arc::new(Provider)))`. Absent provider or returned error yields the exact string `deferred-to-canonical-fallback`; it never grants admission or blocks a committed canonical write.

Temporary minimal `ProjectIdentity` seam: `status(&self, repo: &Path) -> Result<Value>`, registered using `register_project_identity`. Main requested retaining it here and will replace it with the real project_identity::status call at integration. No duplicate identity resolution logic exists.

Reused provider registries: bank_contract::BankObservation/RecoveryCoverage, bank_writes::ScopeBinding/Checkpoint/RecoveryFrontier/RecoveryTransactions/Security/TurnSources.

## Changes to bank_writes traits

Existing method signatures remain unchanged. Added REQUIRED methods, no default bodies:

```rust
// ScopeBinding, baseline context_value(... initialize=...) and require(...):
fn context_value_for_initialization(&self, context: &Value, initialize: bool) -> Result<()>;
fn require_transaction(&self, repo: &Path, project_id: &Value,
    expected: Option<&Value>, owned_lock: Option<&[u8]>,
    owned_transaction: Option<&[u8]>, initialize: bool) -> Result<Value>;
// RecoveryTransactions:
fn apply_transaction(&self, proposal: &Value) -> Result<Value>;
// Security:
fn guarded_start_transaction(&self, repo: &Path, operation_id: &str,
    context: &Value, files: &Value, source_kinds: &[&str], action: &str,
    complete_refresh: bool) -> Result<()>;
// TurnSources:
fn canonical_transaction_barrier(&self, repo: &Path,
    ordinary_bank: Option<&Path>) -> Result<Box<dyn CanonicalWriterBarrier>>;
```

Each trait's provider accessor is directly after its register function and returns `Result<Arc<dyn Trait>>`: scope_binding_provider, checkpoint_provider, recovery_frontier_provider, recovery_transactions_provider, security_provider, turn_sources_provider (all pub(crate)). Existing internal accessors remain private; the new functions forward to the same single registries.

Merged observer lane's REQUIRED RecoveryFrontier additions unchanged:
- observe_owned(repo, owner, transaction) -> Result<Value>
- verified_hold(repo, owner) -> Result<Vec<u8>>
- check(frontier, repo, owner, transaction) -> Result<()>
- require_scope(repo, project, expected: Option<&Value>) -> Result<Value>

Merged bank_contract `BankObservation::scan_sources(&self, bank: &Path) -> Result<(Value, Value, BTreeMap<String, Vec<u8>>)>`: physical path->directory/file/other map INCLUDING runtime entries, hash object, canonical source bytes; baseline scan(read_sources=True, require_router=False). files_at compares both topology maps and hashes across two real scans. Both Main and BankObservation agreed on the final tuple.

## T0 additions

- `Descriptor::create_directory(&self, name: &OsStr, mode: u32) -> io::Result<Descriptor>`: exclusive mkdirat, held descriptor, inode-binding checks, fchmod of only the newly-created inode; never repairs an existing directory. Exact name/signature coordinated with Main and Delivery.
- `Descriptor::remove_directory_at(&self, name: &OsStr) -> io::Result<()>`: unlinkat(AT_REMOVEDIR) on a pinned parent; removes only an empty directory, never recurses.
- Tests exercise modes, EEXIST/non-repair, nonempty removal refusal, symlink refusal and empty removal.

## Argparse declarations for CLI lane

Required subcommand:
- preview: `--repo` default `.`, required `--files` (JSON complete canonical Markdown map), `--operation` choices sorted bank_writes::OPERATIONS, default `compact`.
- apply: required `--proposal`.
- recover: `--repo` default `.`, optional `--mode` choices finish/rollback, optional `--proposal`. Recovery preview requires explicit mode. Prepared recovery uses its own mode.

Success: indent=2, ensure_ascii=False JSON plus newline, exit 0. Refusal: default-spaced JSON result with operation/error/verified=false/inspection_required=true plus bank_capacity details, newline, exit 1. All stderr empty.

## Test coverage / obligations

25 passing tests cover exact golden marker bytes and abc SHA256, private markers/no bodies, exclusive-marker and symlink preservation, oversized marker refusal, partial journal and marker-without-journal evidence, base/final partial-state checks and external read-set edits, a real durable rename with marker still present, exact-journal-only cleanup and unrelated-file preservation, impossible topology/foreign occupants/Unicode/case aliases/runtime descendant symlinks, external read-only state paths, governed rollback rules, source attribution, cache fallback, missing providers, all five missing governed-validator kinds versus raw mechanical operations, native bank flock conflict, T0 directory APIs, unknown/symlink receipts, and CLI error bytes.

With the real BankObservation provider registered (no fake providers), active tests additionally exercise complete UTF-8 file inventory including empty initialization banks and ignored runtime payloads, real replacement/addition/deletion modes and nested canonical directories, whole-read-set mutation immediately before replacement, temporary cleanup, and late alias/unknown marker preservation at the exact before_replace boundary.

Baseline obligation lines in the test file are partial ports where only the independent mechanical branch can execute today:
- test_every_interruption_boundary_refuses_normal_reads_and_converges_in_both_directions: marker sub-contract active; full partial-state converge matrix written/ignored below.
- test_exclusive_marker_creation_preserves_a_new_unknown_marker_and_readset_edit_at_write_boundary: both O_EXCL and real callback-boundary mechanical coverage active; subprocess fault rendezvous deferred to integrated writer.
- test_missing_corrupt_or_changed_journal_is_not_guessed_or_reconstructed: partial/missing journal evidence active; prepared-recovery binding matrix awaits later providers.
- test_external_unchanged_read_set_edit_blocks_finish_and_rollback_without_overwrite: allowed_partial plus real whole-read-set boundary active; complete recovery wrapper deferred.
- test_crash_after_marker_removal_replays_known_outcome_and_purges_leftover_backup: owned-only journal cleanup active; historical apply wrapper deferred.
- test_impossible_projection_and_leaf_directory_conversions_refuse_before_journaling: independent preflight topology fully covered; full CLI path awaits scope/security.
- test_case_unicode_parent_and_existing_occupant_aliases_reject_preview_and_forged_apply_without_loss: independent preflight aliases active; forged full apply awaits scope/security.
- test_noncanonical_physical_blocker_arriving_after_preview_is_checked_before_journal: independent layout blocker checks active; full apply awaits scope/security.
- test_late_alias_at_replacement_boundary_stops_and_recovery_preserves_foreign_occupant: real replacement boundary active; full recovery portion awaits scope/security/frontier/turn_sources.

Written, explicitly ignored at Main's direction:
1. `complete_receipt_schema_and_every_binding_are_required` — `#[ignore = "needs security registered"]`: both terminal outcomes, all twelve required fields individually corrupted/deleted, unknown extra data/sentinel protection. It was run once, and correctly refused on missing SecurityScan (not a product failure or a fake pass). Register real security provider, then remove ignore.
2. `real_partial_rename_converges_finish_and_rollback_at_every_file_boundary` — `#[ignore = "needs recovery_frontier, security registered"]`: actual journals, markers, every boundary of a two-file/router-last transaction in each direction, real lock ownership, convergence readback and cleanup. Register real frontier/coverage/security providers, then remove ignore.

Remaining baseline contract scenarios deferred (not claimed implemented as end-to-end tests) until scope_binding + security + recovery_frontier + turn_sources registrations exist: full preview/apply preserving Git/code state; complete fault-stage matrix including after receipt and after marker removal; stale recovery preview; historical receipt cleanup/no replay; recovery interrupted and resumed; forged/stale proposals through full apply; portable installed copy/data-not-code; raw CRLF through full preview/apply; omitted directory before receipt; arriving aliases after preview/recovery preview; single-file reader interoperability; bank-root spelling through full apply. Production code for those paths is implemented, with no omitted baseline branch, stub or source-authority fallback.

## Declared deltas / safety

- Explicit finite read limits: markers/journals/receipts use the existing 512 MiB private-operation budget; canonical source scan keeps ordinary 16 KiB/aggregate/count bounds. This replaces baseline unbounded marker reads, not canonical-source truncation.
- Non-UTF-8, unsupported BOM/encoding, unsafe symlinks and descriptor/ancestor changes refuse through T0. No path-based std::fs access occurs in the transaction implementation.
- Canonical newly-created parents use explicit 0755 instead of depending on ambient umask; existing modes remain untouched. Canonical file replacements preserve existing ordinary bits or use explicit requested bits/default 0644.
- Map comparisons are order-independent, matching Python dict equality. Rust object ordering remains deterministic for emitted durable JSON through the baseline sorted codec.
- No new child process/deadline paths. Existing opt-in transaction/recovery fault stages call bank_writes::fault unchanged.
- No-panic grep found exactly one production hit: `digest(Some(raw)).expect("present digest input")`, immediately documented invariant that Some input always produces Some; no input-derived unwrap/index/slice panic paths.

## Exact checks and final results

Commands executed in the worktree:
- `cargo test --offline --test t1_bank`: 30/30 after refresh, 30/30 after trait additions, final 30/30.
- `cargo check --offline --all-targets`: final success, zero warnings in owned/shared edited files; seven pre-existing unused-import warnings only in tests/t1_a2.rs (not owned).
- `cargo test --offline --test t1_bank_transactions`: initial 20/20; intermediate compile/test issues fixed; final **25 passed, 2 ignored**, repeated three times consecutively (0.19s, 0.24s, 0.23s).
- `cargo fmt -- --config skip_children=true --emit stdout src/bank_transactions.rs tests/t1_bank_transactions.rs src/bank_contract.rs src/bank_writes.rs src/fs/descriptor.rs src/repository_config.rs`: captured formatter output and applied only changed owned/shared-file lines with edit; did not format unrelated files.
- `cargo fmt --check`: **not globally clean solely because imported read-only observer modules src/bank_observation.rs, src/bank_state.rs, src/tombstones.rs are not rustfmt-formatted**. It reported no diff in bank_transactions, its tests, or the edited shared files. Per file ownership, those imported module implementations were not restyled here; Main/observer lane must format them.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/bank_transactions.rs`: only line 150, invariant digest expect explained above.

No tests requiring later providers were silently narrowed or faked; ignores and deferred obligations are explicit above. Main owns Linux, provider integration, and end-to-end retrying the ignored/deferred tests.
