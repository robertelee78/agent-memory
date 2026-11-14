# Layout recovery observation Rust port

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-layoutobs`. Prerequisites refreshed from committed integration content after `bb77e85` (final snapshot `681c3b7f3e9cff5506709fef7a9a4fb8e4d5b31c`); lane changes retained. No Git writes, Python execution, subagents, awa/Vox/GitHub, or network use.

## Authored files

- `src/layout_recovery_observation.rs` — complete baseline module port.
- `tests/t2_layout_recovery_observation.rs` — real-file contract target.
- `src/lib.rs` — module export.
- `src/error.rs` — append-only `Kind::RawRecoveryCapacity { dimension, limit, measured, exact }` and crate-private constructor.
- `src/cli/mod.rs` — concrete project raw-reader provider registration.
- `tests/wiring-pending.txt` — deletes the project raw-reader pending line.
- `tests/obligations/python-baseline-v1.json` — three ported and two partial obligations, marked with the required am-dev command.
- `tests/t1_project_identity.rs` — Main-authorised removal of one ignore and registration of the real raw-reader provider inside `nested_bank_roots_preserve_raw_recovery_identity`.
- This report, outside the worktree.

## Public API and integration

Constants: `MAX_FILE=1048576`, `MAX_BYTES=67108864`, `MAX_FILES=65536`, `MAX_ENTRIES=131072`, `MAX_DEPTH=64`, `CHUNK=8192`, `MAX_GIT_OUTPUT=67108864`, `SIGNATURE`, `METADATA_POLICY`. Functions: `signature`, `path_value`, `valid_xattrs`, `inode_flags`, `metadata_supported`, `read_source(bank, relative, limit) -> Result<(Vec<u8>, Value)>`, `read_source_hash(bank, relative, limit) -> Result<(String, Value)>`, `locate`, `git_pins`, `identity(repo, raw, review, owner, transaction)`, `inventory`, `inventory_with_options`, `observe`, `recheck`, `capacity_details`.

`InventoryOptions` carries retained directory, prior sources, temporary allowance, envelope, ignored paths. `Inventory` exposes physical_entries/sources/totals. `Totals` and `ManifestBudget` provide exact baseline nested indent-2 JSON accounting. `ObserveOptions` carries owner, transaction, retain (default true), identity_review, predecessor.

`observe(repo, operation_id, runtime_root, runtime_binary, options)` has explicit executing-runtime root and binary parameters: private-state containment uses the same explicit root as layout_recovery_state, and runtime identity uses `artifact::inspect_runtime(runtime_root/skills/agent-memory, runtime_binary)`. There is no environment/global runtime fallback. `recheck(repo, manifest, owner, transaction) -> Result<bool>` is read-only.

`LayoutObservation` implements the existing `project_identity::LayoutObservation`; CLI composition now registers it. No new registration traits or pending registrations. No recovery_frontier module/trait exists in this committed prerequisite snapshot; its future implementation can call the complete exported identity/source/inventory/Git APIs directly. No CLI commands in the baseline module.

## Behavior and durable evidence

Only raw layout observation opts into `ReadOptions::LAYOUT_RECOVERY`: single links, complete parent signatures rooted at the bank, growth classified as lower-bound capacity, capacity-before-binding precedence. Ordinary canonical reads keep their 16,384-byte gate and unchanged-source precedence. Hash-only original-index reads use descriptor chunks, the exact observed index-size bound and all signature/binding rechecks without copying index contents. Git children retain baseline isolated authority and ten-second deadline.

Metadata delegates to the existing T0 flags/xattr/ACL implementation (exact Darwin provenance only; no other xattrs, ACLs or mutable inode flags). Complete physical inventory retains runtime files/directories, omits only the root lock, applies the baseline staging-artifact/unknown-recovery refusals, validates portable aliases, counts ignored physical entries, preserves prior UUID identities and sorts output. Raw evidence is byte-exact (including non-UTF-8 bodies), stored through the real 0600/0700 immutable state writer. Incomplete failures retain already-written originals, never publish completeness, and run the real partial-original-staging fault hook. Observation pins identity, Git/index/configuration, bank binding, metadata and runtime artifact; scans twice, re-resolves identity and rechecks markers. Recheck uses object-order-independent equality for persisted sorted-key manifests.

All baseline functions/branches are implemented; no intentionally unported production branch, placeholder or fake provider. Foundation additions: none. The shared error extension preserves exact capacity details through the existing project reader seam.

## Obligations

Fully ported (exact real counts, no reduced bounds), all in test_layout_recovery.py::LayoutRecoveryInventoryBounds:
- test_actual_aggregate_boundary_preserves_all_sources_on_overflow
- test_actual_source_count_boundary_does_not_drop_or_change_excess_source
- test_actual_physical_entry_boundary_preserves_every_directory_on_overflow

Partial, test_layout_recovery.py::LayoutRecoveryContract:
- test_raw_file_limit_is_inclusive_and_unsupported_aliases_never_hang_or_mutate — complete raw observer limit/type/alias/FIFO behavior; public inspect/receipt path waits for layout_recovery.
- test_raw_depth_boundary_and_unknown_marker_precedence — complete depth 64/65 and real unknown-marker precedence contract; public inspect/output path waits for layout_recovery.

Additional tests cover canonical unsafe paths, supported provenance encoding, all nine signature fields, arbitrary bytes, exact streamed hashes including empty/oversized index, ordinary/raw gate separation, root lock exclusion, runtime/temporary/ignored entries, private originals and stable source IDs, partial staging retention, exact manifest encoder accounting through the actual 512 MiB boundary, malformed prior-ID containment, actual Git configuration/index/head/branch/common pins, no-follow parents, provider wiring, invalid operation precedence, and native Darwin provenance/foreign xattr/ACL/flags refusals.

One compiling integration test is ignored: `complete_observation_retains_private_evidence_and_rechecks_every_pin`, `needs security registered`. It uses only real existing project/developer/bank/raw/runtime APIs and the CLI composition root, no scanner stand-in. It covers full raw observation/staging/recheck, persisted manifest order, changed-byte refusal and index preservation once security is integrated.

Deferred public layout/frontier/recovery journeys (partial-original fault/restart, damaged-evidence refusal through apply, HEAD/index changes after preview, committed review/identity closure, prepared replacement, recovery/restore and provenance restoration) require the not-yet-ported layout_recovery/recovery_frontier/layout_restore machinery. No fabricated or noncompiling ignored test calls those nonexistent APIs; their baseline obligations are not claimed migrated here.

## Declared deltas

- Executing Rust runtime root and binary are explicit; runtime artifact pins the pack plus binary, not the Python-era pack-only digest.
- Git identity/configuration stdout and stderr have an explicit 64 MiB capture bound; baseline capture was unbounded.
- Non-UTF-8 path/Git text is a typed content-free refusal; arbitrary source bytes remain fully accepted. Dynamic malformed manifest shapes return typed refusals rather than Python KeyError/TypeError.
- Prior-source IDs are canonical UUIDv4s before any private blob locator is constructed, closing caller-supplied retention-path traversal; generated/validated baseline manifests are byte-compatible.
- Existing T0 metadata additionally rechecks xattr values/name lists/ACL and ancestor inode bindings before acceptance; no metadata weakening or silent repair.

## Refreshed prerequisites (not independently authored changes)

The following were taken only from committed integration content with `git -C /opt/agent-memory-worktrees/rust-v015-int show HEAD:<path>`, through minimal edit patches (new committed files copied verbatim). Initial snapshot: `7ceed05bea7a38dbb797a22f609efdbda508c2ef`; final snapshot: `681c3b7f3e9cff5506709fef7a9a4fb8e4d5b31c`. Integration owns these versions; do not land them as independent lane changes. Shared-file lane additions and the five obligation marks are the authored deltas listed above. This includes the committed three-target `install.sh` and newly integrated turn_sources/layout_history/private_workspace modules.

- `skills/agent-memory/SKILL.md`
- `skills/agent-memory/access/GUIDE.md`
- `skills/agent-memory/admission/GUIDE.md`
- `skills/agent-memory/assembly/GUIDE.md`
- `skills/agent-memory/authority/GUIDE.md`
- `skills/agent-memory/bank/GUIDE.md`
- `skills/agent-memory/bank/inspect/GUIDE.md`
- `skills/agent-memory/checkpoint/GUIDE.md`
- `skills/agent-memory/comparison/GUIDE.md`
- `skills/agent-memory/consolidation/GUIDE.md`
- `skills/agent-memory/discovery/GUIDE.md`
- `skills/agent-memory/governance/GUIDE.md`
- `skills/agent-memory/identity/GUIDE.md`
- `skills/agent-memory/inactive/GUIDE.md`
- `skills/agent-memory/initialize/GUIDE.md`
- `skills/agent-memory/layout/GUIDE.md`
- `skills/agent-memory/lifecycle/diagnose/GUIDE.md`
- `skills/agent-memory/lifecycle/install/GUIDE.md`
- `skills/agent-memory/lifecycle/migrate/GUIDE.md`
- `skills/agent-memory/lifecycle/purge/GUIDE.md`
- `skills/agent-memory/lifecycle/uninstall/GUIDE.md`
- `skills/agent-memory/lifecycle/update/GUIDE.md`
- `skills/agent-memory/maintain/artifact/GUIDE.md`
- `skills/agent-memory/maintain/cli/GUIDE.md`
- `skills/agent-memory/maintain/pack/GUIDE.md`
- `skills/agent-memory/offline/GUIDE.md`
- `skills/agent-memory/projections/GUIDE.md`
- `skills/agent-memory/records/GUIDE.md`
- `skills/agent-memory/resume/GUIDE.md`
- `skills/agent-memory/retrieval/GUIDE.md`
- `skills/agent-memory/schemas/release-v2.json`
- `skills/agent-memory/scope/GUIDE.md`
- `skills/agent-memory/security/GUIDE.md`
- `skills/agent-memory/sharing/boundary/GUIDE.md`
- `skills/agent-memory/sharing/publication/GUIDE.md`
- `skills/agent-memory/sharing/recovery/GUIDE.md`
- `skills/agent-memory/sharing/refresh/GUIDE.md`
- `skills/agent-memory/sharing/semantic/GUIDE.md`
- `skills/agent-memory/turn/GUIDE.md`
- `skills/agent-memory/writes/GUIDE.md`
- `src/bin/am_dev/release.rs`
- `src/cli/helpers/bank_writes.rs`
- `src/cli/input.rs`
- `src/cli/mod.rs`
- `src/cli/parser.rs`
- `src/cli/surface.rs`
- `src/error.rs`
- `src/installation.rs`
- `src/lib.rs`
- `src/main.rs`
- `src/pack_validation.rs`
- `src/process.rs`
- `src/release_distribution.rs`
- `src/sharing_holds.rs`
- `tests/obligations/python-baseline-v1.json`
- `tests/support/identity_fixtures.rs`
- `tests/support/mod.rs`
- `tests/support/record_fixtures.rs`
- `tests/support/world.rs`
- `tests/t0_process.rs`
- `tests/t1_a2.rs`
- `tests/t2_bootstrap.rs`
- `tests/t2_cli.rs`
- `tests/t2_delivery.rs`
- `tests/t2_sharing_holds.rs`
- `tests/t_support_world.rs`
- `tests/vectors/cli-argparse-v1.json`
- `tests/wiring-pending.txt`
- `.github/workflows/contracts.yml`
- `docs/design/v0.0.15-rust-cutover.md`
- `docs/research/data/2026-11-13-rust-cutover/lane-brief.md`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/clifix-report.md`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/guides-report.md`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/layout-history-workspace-report.md`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/sharingholds-report.md`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/targets-report.md`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/test-world-report.md`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/turn-sources-report.md`
- `install.sh`
- `src/cli/helpers/mod.rs`
- `src/cli/helpers/private_workspace.rs`
- `src/cli/table.rs`
- `src/layout_history.rs`
- `src/private_workspace.rs`
- `src/turn_sources.rs`
- `tests/t1_layout_history_workspace.rs`
- `tests/t1_turn_sources.rs`
- `tests/wiring.rs`

## Verification and command results

Platform actually run: `aarch64-apple-darwin`; Darwin 27.3.0, kernel `xnu-13432.60.57~99/RELEASE_ARM64_T6050`; `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, LLVM 23.1.1. No Linux or cross-target run is claimed by this lane.

Commands run in the lane worktree (repeat executions after committed refreshes retained the authored changes):

```text
rustfmt --edition 2024 --config skip_children=true src/layout_recovery_observation.rs tests/t2_layout_recovery_observation.rs src/error.rs src/lib.rs src/cli/mod.rs tests/t1_project_identity.rs
cargo fmt --check
cargo check --offline --all-targets --features dev-tools
cargo run --offline --features dev-tools --bin am-dev -- obligations mark --status ported --ids-file target/layoutobs-ported-ids.txt
cargo run --offline --features dev-tools --bin am-dev -- obligations mark --status partial --ids-file target/layoutobs-partial-ids.txt
rm target/layoutobs-ported-ids.txt target/layoutobs-partial-ids.txt
cargo test --offline --test t1_project_identity nested_bank_roots_preserve_raw_recovery_identity -- --exact
for run in 1 2 3; do printf '\nlayout observation acceptance %s\n' "$run"; cargo test --offline --test t2_layout_recovery_observation -- --test-threads=4 || exit "$?"; done
cargo test --offline --no-fail-fast
rustfmt --edition 2024 --config skip_children=true src/layout_recovery_observation.rs
cargo fmt --check
cargo check --offline --all-targets --features dev-tools
cargo test --offline --test t2_layout_recovery_observation -- --test-threads=4
grep -n 'unwrap()\|expect(\|\[.*\.\.' src/layout_recovery_observation.rs
```

Final results:
- Formatting clean; offline all-target/dev-tools check clean, no warnings.
- Nested depth-two/depth-three project raw-identity regression: 1 passed (3.08 s).
- Assigned target: **17 passed, 1 ignored** three consecutive runs (58.40 s, 53.55 s, 51.40 s), then the same result in the whole suite and a final direct run (51.65 s). The late bounded-Git-capture addition was rebuilt before the third run; its three consecutive executions (third run, whole suite, final direct run) all passed.
- Source panic scan: no matches. No input-derived unwrap/expect/slicing panic sites.
- After Main-directed final prerequisite refresh including `install.sh`, reran `cargo fmt --check && cargo check --offline --all-targets --features dev-tools`, re-marked the five obligations with the two commands above, and ran **`cargo test --offline --no-fail-fast`: green**. Tool summary: **717 passed, 54 suites, 35 ignored**; final command finished in 327.60 s. Wiring and obligations ledger are green.

Earlier iterations: compilation corrected ordinary Rust API/type mismatches; first test runs corrected independent expected evidence for kernel-issued macOS provenance and sanitized Git effective configuration. An earlier whole suite had exactly one failure in the stale bootstrap's Darwin/x86_64 target selection. Main identified the committed three-target `install.sh`; the final committed refresh resolved it, and the once-rerun whole suite is green. No production retry, weakened assertion, reduced bound or ignored failing test was introduced.

The temporary obligation-ID files were deleted. No throwaway script or Git modification command was used.
