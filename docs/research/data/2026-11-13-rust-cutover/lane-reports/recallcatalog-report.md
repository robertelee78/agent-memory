# recallcatalog lane report

## Result and exact files

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-recallcatalog`, detached integration baseline `681c3b7`. No Git writes, Python execution, subagents, AWA, Vox, GitHub operations, new crates, or non-owned dependency refreshes.

Created:
- `src/recall_catalog.rs`
- `tests/t2_recall_catalog.rs`
- This external report.

Changed shared files:
- `src/lib.rs`: alphabetical `pub mod recall_catalog`.
- `src/cli/mod.rs`: append `bank_transactions::register_index_invalidation(Some(Arc::new(recall_catalog::IndexInvalidation)))` inside `register_providers`.
- `tests/wiring-pending.txt`: remove the now-wired `bank_transactions::register_index_invalidation waits for recall_catalog` line. Other pending registrations are unchanged.
- `src/error.rs`: append `Kind::RecallCatalogMiss` and its crate-visible constructor.
- `src/host.rs`: append `random_32() -> Result<[u8; 32]>`, using two calls to the existing platform kernel-entropy primitive. No new unsafe code.
- `tests/obligations/python-baseline-v1.json`: 14 catalog obligations marked partial and one marked ported using the required am-dev command.

Temporary obligation-ID files `/tmp/recallcatalog-{partial,ported}-ids` were removed after marking. No repository documentation file was added.

## Implementation and public API

All baseline catalog operations are implemented. Python-only `opaque_bindings`, `normalize`, `function_identity`, `class_identity`, and `code_identity` are intentionally replaced, not executed, by the design-mandated runtime artifact generation. There are no implementation stubs, fake providers, migrated cache contents, or silently successful missing-provider branches.

Constants: `SCHEMA`, `MAX_CACHE` (256 MiB), `MAX_HEADER` (16 KiB), `MAX_NAMESPACE` (256), `MAX_POINTER`, `MAX_KEY`, `MAX_RECORD`, `MAX_SOURCE`, `MAX_COLLECTION`.

Public types:
- `Runtime { pack: PathBuf, binary: PathBuf }` supplies the actual runtime inventory roots explicitly.
- `Generation`, constructed by `generation(&Runtime)`, with `value()`.
- `Catalog`, constructed by `load_catalog`, retaining the authenticated metadata, canonical hashes, entries, immutable pointer witness, counters, and generation.
- Unit provider `IndexInvalidation` implements the existing transaction hook.

Public operations:
- `compact`, `generation_details`, `generation`, `coherent`.
- `physical_inventory`, `physical_inventory_with_limit`, `normalized_physical`.
- `locations` and `locations_with_environment` (returns root, bank, cache directory).
- `read_private`, `key(directory, create)`, `binding`, `pointer`, `namespace`, `publication_lock`, `authenticate`, `collect_owned`.
- `invalidate(repo, project, expected_pointer, runtime_root)` is intentionally best-effort.
- `invalidate_after_change(repo, current_hashes, runtime_root)` always returns the baseline state string; errors become `deferred-to-canonical-fallback`.
- `invalidate_after_change_at(root, bank, directory, current_hashes)` is the fallible already-located implementation, used by isolated real-filesystem tests.
- `invalidate_frontier_transition(repo, runtime_root)` retains the strict authenticated-pointer check, directory fsync, and exact transition result.
- `checked_markers`, `load_catalog`, `load_catalog_with_environment`, `publish`, `publish_with_environment`, `optional_failure`.
- `Catalog::selected_entries(selected)` includes correction/supersession dependents and every oversized active record, deterministically deduplicated; `source(path, maximum)` performs a pinned bounded canonical read and hash comparison; `parsed(path, text)` uses exactly the baseline record-schema allowed metadata and verifies the selected format; `recheck(owner)` hashes every canonical source again, checks physical topology/omissions/stamps/frontier markers and engine coherence, returning baseline `payload_bytes`/`file_reads` metrics.

The environment-injection variants accept existing `bank_writes::StateEnvironment`; tests do not mutate process HOME/state environment. Ordinary callers use the current environment wrappers. There is no new CLI subcommand; the Python module has no argparse surface.

## Generation identity and ownership

The engine JSON is exactly `{"artifact_digest": <package::engine_generation(pack,binary)>}`. `package::engine_generation` delegates to fresh `artifact::inspect_runtime`, binding both portable inventory and native binary. Every baseline Python engine (`sha256` plus `modules`, derived from bytecode/interpreter/cache-tag policy) necessarily differs in identity shape, even if its hash text happens to match the artifact digest. Loading an authenticated old engine invalidates only its owned cache witness and returns a cache miss before decoding its payload. Canonical Markdown is neither migrated nor rewritten.

`coherent` freshly checks the runtime artifact at load/publication/final qualification boundaries. An active validation memo disables native catalog reuse/publication, matching the baseline separation from operation-local validation reuse. Tests exercise actual binary replacement/restoration, added pack policy bytes, unsafe pack entries, oversized inventory assets, and non-equality with Python-era engine shapes.

## Registrations and dependency boundaries

No new registration trait or registration function was introduced. The lane implements and wires `bank_transactions::IndexInvalidation`. The provider resolves the already-registered CLI runtime root, invokes best-effort catalog invalidation, and always returns an ordinary JSON state string. Runtime-root resolution and all cache failures become `deferred-to-canonical-fallback`; committed canonical transactions remain successful.

Reused the existing `bank_writes::RecoveryFrontier` provider/accessor with `observe_owned` and `verified_hold` for marker qualification. Its absent registration remains a refusal, never an empty frontier. Security remains in the existing repository-settings/bank-observation seams. Retrieval, discovery and comparison are future consumers of this module, not missing callees requiring duplicate traits. No fake security, frontier, retrieval or discovery provider was installed.

## Filesystem, codec and authentication contract

- Cache schema/path remain `agent-memory/recall-catalog-v1` / `recall-catalog-v1`; cache witnesses remain disposable and non-authoritative.
- Sorted compact UTF-8 finite encoding matches `json.dumps(sort_keys=True,separators=(',',':'),ensure_ascii=False,allow_nan=False)`. Existing baseline last-wins JSON decoding is retained with explicit byte/depth ceilings.
- HMAC-SHA256 is implemented with the existing `sha2` crate using streaming unsigned-header, LF and payload input; no extra whole-payload copy is made for HMAC. A committed-in-test independent OpenSSL vector verifies authentication; forged payload metadata with recomputed digest/length is specifically rejected at MAC verification.
- Every filesystem operation uses foundation trust inspection and pinned descriptors/native rename. No production `std::fs` access or unsafe code was added.
- Private reads require a regular single-link effective-UID-owned record, no group/other access, stable file identity and parent bindings, with named read bounds.
- Corrupt short keys are preserved in private recovery records. Oversized otherwise-private keys are preserved by no-replace native rename without an unbounded read. Unsafe keys are never repaired.
- Publication uses nonblocking native flock without PID markers/recovery; capacity is established before key/generation/pointer creation. Generations are immutable; pointer updates use observed-digest durable replacement. Unknown/corrupt/foreign evidence is preserved.
- Collection charges attempted bounded reads, including refused corrupt records, against the baseline two-cache-size aggregate budget. Authenticated records are deleted only after checking their originally observed inode/signature; cleanup does not reread whole generations outside that budget.
- The physical hint walker charges excluded runtime descendants, refuses symlinks, and has explicit source-count-derived and depth bounds. Stamps never substitute for final whole-content hashing.

## Obligation coverage

Final target: 18 tests, 14 passed, four explicitly ignored. No ignored test is a fake-provider pass.

Fully marked ported:
- `test_hint_inventory_charges_excluded_runtime_entries_against_its_physical_budget`: exact finite physical-budget behavior, including excluded runtime descendants, seven-field stamps and runtime symlink refusal.

Marked partial (exact IDs in the ledger/test annotations; shortened method names here):
- `test_caller_modified_metadata_cannot_forge_a_successful_empty_selection`: independent authentication vector and forged checked/entries payload with freshly recomputed SHA-256/length; full retrieval fallback/rebuild awaits retrieval/discovery/security/frontier.
- `test_actual_forget_removes_owned_generations_before_any_later_recall`: real authenticated owned-generation deletion, unknown/corrupt evidence preservation, unchanged-hash handling and foreign bindings; governed forget/replay/resurrection integration awaits governance and recall providers.
- `test_corrupt_oversized_cleanup_header_defers_without_decoding_it_or_failing_actual_forget`: exact pre-decode header-bound refusal, retained corrupt bytes and non-failing transaction-hook fallback; actual governed forget/replay awaits governance/security/frontier.
- `test_full_namespace_preserves_unknown_evidence_and_stops_adding_cache_files`: 256 unknown files preserved byte-for-byte, no new optional lock, over-bound inventory refusal; useful canonical retrieval under exhaustion awaits retrieval/discovery/security/frontier.
- `test_busy_optional_publisher_leaves_useful_recall_available_and_rebuilds_after_unlock`: real nonblocking kernel flock, release and reacquisition without PID files; retrieval/rebuild result parity awaits retrieval/discovery/security/frontier.
- `test_changed_executing_package_rebuilds_instead_of_reusing_an_old_engine_witness`.
- `test_actual_loaded_policy_code_namespace_and_dependency_changes_invalidate_the_witness`.
- `test_warm_recall_refuses_changed_identity_tokens_defaults_and_callable_attributes`.
  These three cover the approved Rust replacement identity (native binary/pack artifact) rather than impossible mutable Python runtime bindings; actual retrieval fallback/rebuild remains unavailable.
- `test_unavailable_bounded_boot_manifest_disables_only_the_optional_accelerator`: real unsafe/oversized artifact inventories and validation-memo separation refuse optional generation without changing runtime bytes; useful retrieval result preservation awaits retrieval/discovery/security/frontier.
- `test_oversized_canonical_source_refuses_with_or_without_catalog`: real canonical scan enforces source byte-capacity independently of caches; both public retrieve modes await retrieval/discovery/security/frontier.
- `test_warm_semantic_selection_matches_cold_sources_without_storing_learning_bodies`: concrete publish/load/source/final-recheck/body-exclusion test retained with `#[ignore = "needs recovery_frontier, security, retrieval, discovery registered"]`; full semantic-record result parity still needs the consumers.
- `test_missing_cache_automatically_rebuilds_and_returns_the_same_useful_memory`: concrete missing-load/publication/reload/hash/final-recheck test retained with the same explicit ignore; automatic retrieval fallback needs consumers.
- `test_late_mmap_change_with_restored_mtime_cannot_pass_the_final_whole_content_check`: concrete late same-length source-edit/recheck/pointer-removal test retained with `#[ignore = "needs recovery_frontier, security registered"]`; the original subprocess mmap/restored-mtime rendezvous qualification is deferred.
- `test_loaded_immutable_reader_survives_concurrent_generation_replacement`: concrete retained-reader/publication replacement/owned-collection/recheck test retained with the same frontier/security ignore; original multi-process retrieval rendezvous awaits consumers.

Additional passing tests cover private modes, hard links, symlinks, FIFO nonblocking refusal, exact read boundaries/missing records, malformed/oversized pointers, key recovery, HMAC field binding and tampering, unchanged-hash state, foreign owned-evidence preservation, and the native artifact generation shape.

Three remaining baseline end-to-end methods remain unported in the ledger (not silently claimed covered):
- `test_ordinary_recall_preserves_optional_navigation_catalog_without_republishing_it`: retrieval ordinary-vs-navigation behavior, waits for retrieval/discovery/security/frontier.
- `test_common_installed_cli_reuses_the_same_validated_catalog`: installed search handler and normal production recall wiring, waits for retrieval/discovery/security/frontier and installed-CLI integration.
- `test_warm_policy_matches_full_reader_for_hidden_unsafe_and_conditional_corrections`: complete retrieval lifecycle/applicability/correction selection matrix, waits for retrieval/discovery/security/frontier.

These are explicit qualification dependencies, not omitted catalog implementation branches.

## Declared deltas

- Approved engine-identity cutover described above replaces all Python execution-policy introspection; no compatibility shim or old-generation migration exists.
- Foundation Rust JSON policy refuses unsupported encodings/BOM/lone surrogates and nesting beyond 256. Typed inputs eliminate Python dynamic-type exceptions. Invalid metadata remains a cache miss/refusal, not a success.
- Finite directory depth 256, bounded descriptor-relative inventories, full record signature/parent checks and no-replace oversized-key preservation are deterministic safety tightening. Physical traversal/object order is deterministic rather than filesystem enumeration order.
- Runtime roots and binary paths are explicit typed inputs, not inferred Python source paths. The transaction provider uses the normal CLI runtime-root resolver and safely defers if unavailable.

## Commands, platform and outcomes

Platform: `aarch64-apple-darwin`; Darwin 27.3.0 (`Darwin Kernel Version 27.3.0: Fri Oct 30 02:01:13 PDT 2026; root:xnu-13432.60.57~99/RELEASE_ARM64_T6050`); rustc 1.99.1 (`58cf8f9c3`, 2026-10-13), LLVM 23.1.1. Linux/musl checks were not run.

Commands run in this worktree:
- Independent vector computation with `printf`, `shasum -a 256`, and `/usr/bin/openssl dgst -sha256 -mac HMAC -macopt hexkey:000102...1f`: payload digest and MAC captured in the test constant.
- `uname -a`; `rustc -vV`: platform above.
- `cargo run --offline --features dev-tools --bin am-dev -- obligations mark --status partial --ids-file /tmp/recallcatalog-partial-ids`: success, 14 IDs.
- Same command with `--status ported --ids-file /tmp/recallcatalog-ported-ids`: success, one ID.
- `cargo fmt --check`: final clean. Owned files were formatted by capturing `rustfmt --edition 2024 --emit stdout` and applying only its diff via the edit tool; no formatter wrote unrelated files.
- `cargo check --offline --all-targets --features dev-tools`: final success, no warnings.
- `cargo test --offline --test t2_recall_catalog`: final 14 passed / 4 ignored, three consecutive passes after the last test-only edit (0.15s, 0.10s, 0.15s). Earlier completed three-pass sequence also passed before replacing the FIFO fixture's unbounded direct subprocess with the foundation five-second bounded subprocess.
- `cargo test --offline --no-fail-fast`: success, all executed tests passed; tool summary reports 713 passing tests across 54 suites, 39 ignored (spawned filtered worker invocations included in the summary). Includes passing obligation-ledger and wiring tests. This suite run compiled before the final test-only FIFO process-supervision improvement; production code was unchanged, and the final target/check/format sequence above verified that improvement separately.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/recall_catalog.rs`: no matches (grep exits 1 for no matches). Thus the enclosing shell command's final exit 1 is the clean panic grep, not a cargo failure.
- Removed the two temporary obligation-ID input files.

Earlier fixed development failures: one fixture stamp expected 28 rather than the actual 29 bytes, an unused test import, and the independent digest test attempted LowerHex on this sha2 version's digest array. Formatting-anchor reads were required before applying the formatter diff. No final failures or warnings remain.
