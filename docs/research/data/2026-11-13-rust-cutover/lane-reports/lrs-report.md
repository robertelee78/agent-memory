# Layout recovery state Rust port

## Delivery

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-lrs`.
Complete baseline `layout_recovery_state.py` port; no unported functions or branches. All 16 tests pass three consecutive times after refreshing integration prerequisites. Main received `layout_recovery_state ready` after compilation and the first two passing tests.

Authored files to integrate:
- `src/layout_recovery_state.rs` (new).
- `tests/t1_layout_recovery_state.rs` (new).
- `src/lib.rs`: alphabetical `pub mod layout_recovery_state;` registration only, on top of integrated registrations.
- `src/local_registry.rs`: Main-approved explicit-bound replacement addition, on top of the refreshed integrated version. `replace` and `replace_with_sources` retain the 64 MiB default; new `replace_with_limit` and `replace_with_sources_and_limit` share the existing implementation and parameterize only its existing-generation bounded read. The recovery module calls `replace_with_limit(..., sharing_limits::MAX_PRIVATE)`. No default was raised.
- This report: `/Users/robert.lee/.local/state/agent-memory/research/rust-cutover/lrs-report.md`.

No T0/error/Cargo behavior changes were authored. No Git writes, Python execution, network access or subagents.

## Public API

All operations return the crate's typed `Result` except predicates/signature.
- `BINDINGS`, `INDEX` constants.
- `sha(&Value) -> bool`; `signature(&fs::Identity) -> fs::Identity` includes all nine baseline fields.
- `read_bytes(path, maximum: usize, missing: bool) -> Result<Option<Vec<u8>>>`.
- `read(path, maximum: usize, missing: bool) -> Result<Option<Value>>`.
- `private_parent(path) -> Result<()>`.
- `put_bytes(path, &[u8])`, `write_once(path, &Value)`, `replace(path, &Value, expected_sha256: Option<&str>) -> Result<String>`.
- `bindings() -> Result<Value>`.
- `RecoveryPaths = (PathBuf, PathBuf, PathBuf, Option<PathBuf>)`.
- `paths(repo, operation_id: Option<&str>, runtime_root)` and `_paths_for_bank(root, bank, operation_id, runtime_root) -> Result<RecoveryPaths>`.
- `index(repo, runtime_root)` and `_index_for_bank(root, bank, runtime_root) -> Result<Value>`.
- `set_index(repo, value, expected_sha256, runtime_root) -> Result<String>`.
- `bind(repo, operation_id: &str, project_id: &str, checkout_key: &str, lifecycle_state: &str, runtime_root) -> Result<Value>`.
- New registry APIs: `replace_with_limit(registry, payload, observed, limit)` and `replace_with_sources_and_limit(registry, payload, observed, sources, limit) -> Result<()>`.

**Runtime root is explicit**, replacing Python's `Path(__file__).resolve().parents[2]`; it is resolved before the outside-runtime check. There is no Rust source-of-truth global and no environment/global fallback.

No new traits or registration points. `bank_contract::bank_root` and `repository_config::recovery_settings` are called directly. The public-path fixture supplies the existing `repository_config::SecurityScan` seam only for its asserted literal `memory-bank`; this does not implement or test the later security module.

No argparse declarations: the baseline module has no CLI.

## Filesystem/codec parity

The read path runs baseline trust inspection, pins its parent directory, opens the record no-follow/nonblocking, checks effective owner, exact 0600, regular type, one link and explicit size bound, reads bounded bytes, compares full file signatures against the opened and named inode, and rechecks the named parent's device/inode. Writes use exclusive descriptor-relative creation, 0600, fsync of file and directory, and checked readback. Private directories retain exact 0700 requirements without repairing existing modes. Canonical records use existing `developer_identity::encoded` bytes and strict decoding. Mutable replacement retains digest comparison and readback checks. Existing refusal messages, path/binding/index shapes and lifecycle sets are preserved.

`ReadOptions::LAYOUT_RECOVERY` is not used: the available T0 documentation describes layout-recovery observation capacity behavior, not this state module. Its full-parent-signature/growth-as-capacity behavior does not match this module's baseline (parent inode binding and changed-source refusal). The module uses the existing lower-level descriptor primitives instead.

The stricter Rust `bank_writes::state_paths` validates UUIDs before locating paths; an invalid supplied recovery identifier therefore uses a fresh locator UUID for the location/binding guards, then receives this module's own exact baseline UUID refusal. No invalid identifier is published.

## Tests and obligations

16 real-file tests cover canonical UTF-8 sorted/indented/newline bytes, immutable replay/foreign-byte refusal, missing records, digest compare-and-replace, exact file/directory modes without chmod repair, second hard links, symlinks, all signature fields, bounds, non-canonical/duplicate/BOM/non-finite JSON, binding schemas and identities, pathlib-equivalent lexical normalization, redirected/moved binding paths, state-root moves, active missing indexes, locked bindings, bank/runtime/UUID guards, index identity/generation fields and every allowed lifecycle, public `bind` lifecycle-only updates/identity refusal, and a real concurrent parent-directory replacement during a bounded 128 MiB observation.

Two replacement-bound regressions cover both explicit small bounds and an actual sparse existing record of 64 MiB + 1 byte: ordinary `local_registry::replace` refuses and leaves it untouched, while `layout_recovery_state::replace` succeeds within MAX_PRIVATE, verifies its successor bytes and leaves no temporary files. The oversize immutable input test refuses MAX_PRIVATE + 1 before creating its parent.

No standalone baseline contract test exercises only this module and already-ported dependencies: the matching contract suites require recovery/frontier/admission/recall/layout modules. No whole-suite obligation is falsely claimed migrated. New tests directly cover this module's full baseline contract and the assignment's refusal cases.

Deferred integration obligations (not implementation gaps):
- `tests/contracts/test_layout_damaged_evidence.py::LayoutDamagedEvidence.test_original_evidence_refuses_changed_or_missing` and `.test_review_refuses_changed_or_missing`: layout recovery inspection/prepare/apply/recover and retained-evidence validation (T2d).
- `tests/contracts/test_layout_recovery_identity_closure.py::RecoveryIdentityClosureContract.test_head_or_index_change_after_preview_refuses_before_bank_mutation`: layout recovery preview/apply (T2d).
- The same suite's `.test_changed_state_or_config_homes_cannot_hide_public_apply_hold` and `.test_deleting_every_discovery_record_loses_the_pending_obligation`: recovery_frontier, scope/project binding and recall (T1/T2a/T2d).
- Direct fixture consumers in held-source enrollment, OMP enrollment/source-phase/writer-fence and hook-failure suites wait for their corresponding downstream modules. Their full command-level obligations are not narrowed to this storage module.

Declared deltas: typed Rust bytes/paths replace Python dynamic argument types; unsupported argument types are unrepresentable. JSON is strict UTF-8, rejects BOM/UTF-16/UTF-32/lone-surrogate/depth-overflow inputs per the shared codec policy. Non-string binding paths produce the existing moved/redirected typed refusal instead of Python TypeError. Root containment uses the explicit executing-runtime parameter noted above. No on-disk format delta.

Panic audit: `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/layout_recovery_state.rs src/local_registry.rs` reports only the recovery module's `digest(Some(raw)).expect(...)`; the immediately preceding invariant comment establishes in that function that passing Some always produces Some. No other hits in either production file.

## Verification

Final successful commands, run inside the lane worktree:
- `cargo fmt --all` (formatting), then `cargo fmt --all --check`: clean.
- `cargo check --offline --all-targets`: success, no warnings.
- `cargo test --offline --test t1_layout_recovery_state`: 16 passed, 0 failed, three consecutive runs (0.98 s, 1.06 s, 1.07 s test times).
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/layout_recovery_state.rs src/local_registry.rs`: single justified invariant hit above.

Earlier development checks: the first all-target check exposed the missing Value::as_object_mut adapter and was fixed; the first two-test run exposed order-sensitive Rust Value equality in the test assertion and was fixed to compare canonical bytes. The initial 2-test and later 12-test targets then passed. After source refresh, the first all-target check exposed stale non-owned tests, which were refreshed; the next exposed their missing release-v2 schema, which was refreshed. Final results above are after those fixes. No other suites were executed.

## Integrated prerequisites refreshed (not independent authored changes)

Per Main's instruction, non-owned sources were refreshed from `/opt/agent-memory-worktrees/rust-v015-int` before final verification. Integration already owns these versions; do not re-integrate them as new lane changes.

Existing refreshed paths: `Cargo.toml`, `Cargo.lock`; `src/{artifact,authority,developer_identity,error,handoff,host,lib,lifecycle,local_registry,pack_validation,process,records,repository_config,scopes,sharing_limits}.rs`; `src/fs/{descriptor,lock,resolve}.rs`; `tests/{t0_fs,t0_host,t0_unicode,t1_a1,t1_a2}.rs`; `tests/obligations/python-baseline-v1.json`.

New integration prerequisites copied: `src/{cache_recovery,console,main,project_identity,release_distribution}.rs`; `src/bin/am_dev/{main,obligations,release}.rs`; `src/cli/{helpers,mod,input,parser,surface,table}.rs` except helpers is a directory containing `{artifact,authority,bank_contract,bank_layout,bank_writes,developer_identity,lifecycle,mod,pack_validation,records,scopes}.rs`; `tests/{obligations_ledger,t1_project_identity,t2_cli,t2_delivery}.rs`; `tests/vectors/cli-argparse-v1.json`; `skills/agent-memory/schemas/release-v2.json`.
