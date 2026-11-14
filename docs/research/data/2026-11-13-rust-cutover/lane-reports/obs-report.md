# Observation lane report

## Implementation and files

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-obs`.

All baseline functions/branches in `bank_observation.py`, `bank_state.py`, and `tombstones.py` are implemented as typed Rust operations, with the explicit registered dependencies below. There are no implementation stubs or silent unregistered-provider fallbacks. CLI argument parsing stays with the CLI lane.

Exact authored/hand-off file list:
- `src/bank_observation.rs` (new)
- `src/bank_state.rs` (new)
- `src/tombstones.rs` (new; includes four private-helper unit tests)
- `tests/t1_bank_observation.rs` (new; 32 integration tests)
- `src/lib.rs` (three alphabetical module declarations)
- `src/bank_contract.rs` (authorized shared trait extension, scan forwarder, coverage-provider accessor)
- `src/bank_writes.rs` (authorized shared frontier methods and provider accessor)
- `src/repository_config.rs` (authorized extension of the existing security scan seam and accessor)
- this report, outside the worktree.

Before authoring, refreshed differing dependencies under `src/`, `tests/`, and Cargo manifests from Main's integration worktree as directed (7622e91-era fixed dependencies). Those inherited coordinator changes are not this lane's patch; in particular this lane added no T0 primitive, error variant, dependency, or Cargo manifest change. The refresh also brought in `omp_installation` and its integration tests. Do not replace integration-owned files wholesale with this lane's copies; reconcile only the shared additions listed below.

## Public API and registration

`bank_observation`: `read_source`, `markers`, `scan`, `scan_with_limits`, `physical_inventory`, `physical_contract`, `observe`; `ScanLimits`, `Scan`, `Physical`, invocation-local `Observation` with `recheck(stage)` and `into_contract()`; unit provider `BankObservation`.

Required observer registration:
```rust
bank_contract::register_bank_observation(Some(Arc::new(bank_observation::BankObservation)));
```
`scan_sources` returns `(physical_object, hashes_object, source_bytes_map)`, requires no router, and includes runtime entries in the physical object. `scan_hashes` likewise requires no router, matching baseline `bank_writes.hashes`. Canonical source reads use T0's default `growth_is_capacity = false`: growth is source changed. Stable per-source capacity is exact; aggregate and partial count capacity are lower-bound observations. Unsafe source/parent checks precede capacity classification; count wins over aggregate after a safely checked excess source.

`bank_state`: `git_context`, explicit-environment `git_context_with_environment`, `inspect`, `inspect_context`, `command_inspect`, `command_result`. One upward trait definition, `ComparisonView`, with `observed_context(root, request) -> Result<(Value, Value)>`; registered using `register_comparison_view(Some(Arc::new(retrieval_provider)))`. Recall/retrieval implements it. Other comparison/frontier/layout methods must extend this same definition rather than introducing another `ComparisonView`.

`tombstones`: schema alias `SPEC`, `PREFIX`, `value`, `relative_path`, `parse`, `render`, `validate`, `canonical_paths`, `history`, `filter_projection`, `command_result`; `History` tuple; unit provider `TombstoneParser`.

Required tombstone registration:
```rust
records::register_tombstone_parser(Some(Arc::new(tombstones::TombstoneParser)));
```

## Changes to shared traits

All additions are required methods, without default implementations. Existing methods were not reordered or changed.

`bank_contract::BankObservation`:
```rust
fn scan_sources(&self, bank: &Path)
    -> Result<(Value, Value, BTreeMap<String, Vec<u8>>)>;
```
Public matching forwarder `bank_contract::scan_sources(bank)`. Added `pub(crate) fn recovery_coverage_provider() -> Result<Arc<dyn RecoveryCoverage>>` immediately after its registration function.

`bank_writes::RecoveryFrontier` (future implementer: recovery_frontier):
```rust
fn observe_owned(&self, repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>) -> Result<Value>;
fn verified_hold(&self, repo: &Path, owner: Option<&[u8]>) -> Result<Vec<u8>>;
fn check(&self, frontier: &Value, repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>) -> Result<()>;
fn require_scope(&self, repo: &Path, project: &Value, expected: Option<&Value>) -> Result<Value>;
```
`verified_hold` returns the baseline verified hold object's `marker` bytes. `observe_owned` is the owner-aware baseline observe operation; the existing ownerless `observe(repo)` remains unchanged. Added `pub(crate) fn recovery_frontier_provider() -> Result<Arc<dyn RecoveryFrontier>>` immediately after registration.

Reused `repository_config::SecurityScan` (future implementer: security), appending:
```rust
fn scan(&self, body: &str) -> Result<Value>;
```
Existing `has_reasons` remains. Added `pub(crate) fn security_scan_provider() -> Result<Arc<dyn SecurityScan>>` immediately after registration. This avoids a duplicate scanner trait. `bank_state` imports no second scope/security seam: scope checks run through recovery_frontier's baseline `require_scope`; the Python `scope_binding` lazy import itself has no independent call.

Recovery coverage still registers through `bank_contract::register_recovery_coverage`; no trait change there. Direct calls reuse `local_registry`, `developer_identity`, `handoff`, repository configuration, and the existing T0 codecs/FS/process APIs.

## CLI declarations and outcomes

- bank_state: required subparser `inspect`; `--repo` default `.`; optional `--path`. Typed `command_inspect` returns exit 0/1, value, stdout, empty stderr. Success uses indent=2/ASCII JSON; refusal uses default JSON plus capacity details.
- tombstones: required subparser; `render --input REQUIRED`; `validate --file REQUIRED`; `filter --repo . --input REQUIRED`. CLI supplies bounded/decoded inputs to the typed operations. Success uses indent=2/UTF-8 JSON. Non-capacity errors retain the exact generic content-free refusal string; capacity errors retain structured details and the body-free capacity message. Exit 0/1, trailing stdout LF, empty stderr.
- bank_observation has no argparse surface.

## Obligation coverage

Exact obligation annotations are retained above tests. The distinction below is intentional: no fake frontier, scope, security, retrieval, checkpoint, or ownership provider was installed to turn unavailable cross-domain tests green.

Fully ported local/pure contracts:
- `test_tombstones.py::TombstonesContract.test_exact_five_field_format_roundtrips_without_personal_prose_or_generated_state`
- `test_tombstones.py::TombstonesContract.test_sensitive_extra_fields_bodies_names_and_fingerprints_refuse_without_echo_or_mutation`
- `test_tombstones.py::TombstonesContract.test_every_physical_history_occupant_is_checked_before_returning_stale_content` (all nine original variants plus symlink; directly exercises canonical history inventory).
- Successful five-field parsing/record dispatch for `test_records.py::RecordContract.test_copied_artifact_and_five_field_tombstone_exception_require_no_harness_or_state`, closing the successful parser cases deferred in t1_a2; copied package executable integration remains the distribution lane's concern.
- Exact UTF-8 file boundary, known SHA-256 vectors, stable reported-size refusal, count/aggregate precedence, real 64 MiB aggregate overflow, unsafe excess source, parent no-follow, descriptor release, owned marker equality, physical/runtime topology, and typed command capacity/redaction are exercised on real private filesystem fixtures.

Partially ported cross-domain obligations:
- Four `test_tombstones.py` obligations (`test_delete_rebuild_and_repeat_cannot_resurrect_id_under_changed_conditions`, `test_unknown_ids_wrong_project_invalid_history_and_interrupted_bank_stop_rebuild`, `test_history_namespace_route_state_and_record_identity_must_match_before_rebuild`, `test_copied_portable_guard_uses_the_same_canonical_project_history`) have four green unit tests over real canonical scans, real router/record validation, verified source hashes, actual history reads, projection suppression, route/scope/ID mismatches, copied canonical history, and interruption-marker refusal. Their public `history`/`filter_projection` snapshot wrappers still need concrete recovery_frontier and security registrations. No copied executable installation is claimed.
- `test_bank_layout.py::LayoutContract.test_tombstones_and_duplicate_record_versions_cannot_hide_or_recreate_current_content`: the integration target now proves a safe tombstone bank validates through the real registered parser, resurrection/duplicates refuse, and non-Markdown history occupants refuse. Its public inspector wrapper awaits domain registrations.
- `test_bounded_bank_observation.py` coverage uses real scans/markers/pinned reads. `test_complete_utf8_source_boundary_and_file_only_validator` and `test_public_snapshot_refuses_real_aggregate_excess` cover the byte/bank scan portions, not successful public snapshot wrappers. `test_complete_map_count_is_exact_and_partial_inventory_is_at_least` covers partial inventory here; complete-map checks remain in t1_a2. The two reused-parent race obligations, oversized-before-recheck replacement, and owned-marker replacement are exercised by deterministic T0 pinned-read replacement, not Python syscall monkeypatching or a mid-scanner syscall interception. The `fdopen` construction-failure obligation is represented by Rust RAII/unrelated-descriptor preservation; no Rust buffered-stream constructor exists to monkeypatch. Unknown marker refusal/content preservation is tested, but no syscall hook claims to prove absence of opening.
- `test_current_bank.py::CurrentBankContract.test_missing_git_still_reads_the_copied_portable_markdown_bank`: missing-Git status/reason is covered with an isolated PATH; full copied-bank inspect awaits the providers below.

Explicitly deferred integration coverage:
- `BoundedBankObservation.test_changed_source_refused_at_recheck`: the production `Observation::recheck` implementation is complete, but successful owner-aware observation and injected recheck integration wait for recovery_frontier and security.
- All full current-bank inspector scenarios: poisoned/removed application state, six tracked contexts and fresh clone, trailing-space Git root, interrupted-bank state loss, ambient Git redirection/fsmonitor/trace protection, and the full missing-Git copied-bank scenario. Depend on concrete recovery_frontier/security; historical `--path` additionally needs scope/retrieval ComparisonView and checkpoint for activeContext handoff assessment. Optional missing/non-checkout Git branches are green without those providers. The production Git invocation already uses the shared sanitized environment, explicit read-only options, finite ten-second deadline, and bounded captures.
- Public tombstone filter/snapshot and ordinary successful observer wrappers, and nonempty recovery hold/coverage branches: wait for recovery_frontier, RecoveryCoverage, security, and their normal CLI registrations. These are test dependencies, not omitted implementation branches.

## Declared deltas

- Deterministic lexicographic physical/hash ordering replaces incidental Python filesystem/dict traversal order. Both scans produce the same stable object order, including runtime entries.
- All source and history reads are named/bounded and no-follow; canonical sources reject non-UTF-8 instead of inheriting interpreter decoding behavior.
- Optional Git stdout and stderr each have the named 16 MiB `MAX_GIT_CONTEXT_OUTPUT` capture bound. Overflow produces optional-Git unavailable status rather than an unbounded capture.
- Physical inventory has the existing finite 2×source-count budget and explicit depth 256; history uses that complete physical walker, including runtime names, rather than unbounded rglob.
- For stable sources already larger than the remaining aggregate budget, T0 verifies the pinned source/parents before returning size information; the scanner emits the baseline at-least aggregate result without allocating/reading an unnecessary doomed prefix. Source/parent substitution and growth remain unsafe-source refusals, not capacity.
- Rust uses descriptor ownership/RAII, with no Python `fdopen` ownership failure mode.

## Commands and final results

All commands ran only in this worktree; no Python, network, Git writes, or subagents.

- Coordinator prerequisite: `cargo test --offline --test t1_bank` — 30/30 passed after dependency refresh, before shared trait additions.
- `cargo fmt --all` — completed.
- `cargo fmt --all -- --check` — clean (final run).
- `cargo check --offline --all-targets` — clean, no warnings (final run).
- `cargo test --offline --test t1_bank_observation` — final 32/32 passed three consecutive times after the last source edit.
- `cargo test --offline --lib tombstones::tests` — 4/4 passed; also passed three consecutive earlier runs, and passed again after the final observation-only edit.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/bank_observation.rs src/bank_state.rs src/tombstones.rs` — no hits. No panic-on-input exceptions to justify.

Earlier compile/test failures (fixed): slice-to-Vec conversion; restoring the tombstone trim import after inventory refactoring; macOS Unix-socket temporary path length; and the isolated missing-provider test correctly observing the security seam refusal before frontier resolution. No final failures remain in the checks above. Linux and complete cross-domain/CLI qualification remain Main's integration verification.
