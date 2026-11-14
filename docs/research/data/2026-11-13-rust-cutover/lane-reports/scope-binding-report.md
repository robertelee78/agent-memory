# Scope binding lane report

## Delivery and files

Worktree `/opt/agent-memory-rust-scope`, immutable base `2a12544a8e82e1e922f814f8c27540df471cffa0`, branch `lane/scope-binding`.

Exact authored files:

- `src/scope_binding.rs` (new)
- `src/cli/helpers/scope_binding.rs` (new)
- `tests/t1_scope_binding.rs` (new)
- `tests/t2_cli.rs`: owner-authorized followup replaces stale root choice-list expectations with ordered public command-surface names; actual-process exit-2, exact usage/program/refusal/quoting assertions remain
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/scope-binding-report.md` (this required report)
- `src/lib.rs`: one `pub mod scope_binding;` line
- `src/bank_writes.rs`: append-only required trait methods listed below
- `src/error.rs`: append-only `Kind::IdentityUnresolved`, `Kind::SourceTransportBusy { initializing: bool }`, and two crate-private constructors at the end of `impl ContractError`
- `src/cli/table.rs`: exactly one `identity` route line
- `src/cli/helpers/mod.rs`: one `pub mod scope_binding;` line, expressly authorized by Main/owner
- `src/cli/mod.rs`: register the production scope adapter at the end of the existing composition function; change the preceding terminal registration expression to `?` and return `Ok(())`, expressly authorized by Main/owner
- `tests/obligations/python-baseline-v1.json`: parent marked the 13 authored baseline IDs partial with `am-dev`; none is claimed fully ported.

No other source files changed. In particular, no edits to project identity, foundation, Cargo manifests, Python or transaction tests. No transaction-test ignore was removed: both existing ignored tests need security/frontier, not scope alone. The implementation worker performed no commit/push, Python execution, new crate, build script, network call or environment mutation; parent owns ledger and delivery operations.

Handoff identifier files (outside the repository): `/tmp/scope-binding-partial.ids` and `/tmp/scope-binding-deferred.ids`.

## Baseline implementation and API

Every domain function/branch of the 514-line `scope_binding.py` is represented. There are no implementation gaps concealed as test partials. Unavailable callees are required registered interfaces, not substitute implementations.

- `result`, `principal`, `status`: baseline scope receipts; independently reported developer/project failures; project capacity errors propagate; readonly registry-lock reporting; attribution conflicts and locks refuse principal resolution with `IdentityUnresolved`.
- `require(repo, project_id, RequireOptions)`: all initialization, expected-context, owned-lock, owned-transaction and migration-reader behavior. Initialization requires exactly the single pending-initialization issue and an explicitly registered empty checkout. Migration cannot initialize/rebind identity. Context comparison is recursively object-order-independent.
- `raw_recovery_context`: accepts only `&project_identity::RawIdentityEvidence`, never JSON; checks checkout root and uses the real owner/transaction/raw-evidence discovery options before validating an ordinary pinned scope.
- `capture`: unresolved previews remain advisory `Value::Null`, while capacity errors propagate.
- `context_value`: exact seven-field ordinary/initialization contexts, canonical UUID checks, exact boolean initialization purpose; `Null` remains permitted as the advisory absence representation.
- `identity_files`: preserves context prose, record IDs/provenance/conditions/body, index content and tombstone identity; changes only router/record/index project namespace and five-field tombstone `scope`; validates both original and rewritten complete banks.
- `preview`, `bundle_value`: real project preview and complete observed bank snapshots, namespace-only desired files, optional bind transaction, exact bundle fields, matching project/router/base/transaction evidence and initialization-only create rules.
- `binding_guard`: fresh developer and project registry/repository checks, registered operation digest, original proof comparison, expected project IDs and exact historical-device alias repair exception.
- `apply`: original developer/read-set checks, required started audit, exact persisted `binding.json` decision before registry mutation, project replay, `binding-after-registry` fault, current binding guard, journaled trusted transaction apply or prepared finish recovery, exact marker checks, contentful bundle cleanup only after verified transaction, current status readback, initialization-only handoff, completed audit only for verified outcomes. No cross-store atomicity is claimed.
- `command_result`: success `INDENT2_UTF8`, refusal `DEFAULT`, capacity details, baseline inspection/verified fields and exit codes.

`ScopeBinding` implements every method of the actual base `bank_writes::ScopeBinding`, including the pre-existing transaction/initialization additions and the jointly agreed enrollment-locator addition. `register_providers()` installs this real adapter; the CLI composition root calls it. Scope functions themselves do not register missing safety dependencies opportunistically.

## Live private enrollment

`PrivateEnrollmentContext` has private fields and a private unforgeable `Rc` seal. It is not `Clone` or `Send`, has no decoder, serializer, mapping implementation, or conversion into write authority. Scope getters return advisory copies only. The ordinary scope guard never accepts this context type.

`private_enrollment_context(repo, runtime_root)` performs fresh enrollment-location suppression, real grouped settings observation, a registered source-marker observation guard, ordinary/raw bank agreement, canonical-writer contention check, source-compatibility check, full verified hold or ordinary require, private frontier observation, descriptor-derived root/parent/bank ownership/mode/inode pins, exact marker digest and (for holds) runtime/bindings/journal pins, and final ordinary/raw locator checks. Busy and initializing contention unwind all guards before conversion into `SourceTransportBusy`; its fixed-size flag retains which cause occurred without allocating a recursive error enum.

`recheck_private_enrollment` independently remints and compares scope, complete hold, pins and digest. `within` performs successful entry/exit rechecks. `private_enrollment_reads`, `enrollment_read_context`, `fresh_enrollment_locations`, `observe_enrollment_locations`, and `observe_enrollment_location` reproduce the synchronous read/locator context barriers. RAII restores previous thread-local state on refusal/unwind. Locator reuse requires the exact active seal/root/runtime and the already-held source barrier; it never reuses ordinary write scope. No active locator means `None`, as in the baseline.

`VerifiedEnrollmentHold` is a non-JSON typed observation with private fields and immutable getters. `from_verified_observation` is for the trusted recovery-frontier provider after its full source/ledger/runtime/attestation/identity validation, not for loading request JSON. Its data cannot directly authorize bank mutation. `SourceMarkerObservation` guards must restore previous provider observation state on Drop, including failed observations.

## Shared traits and production dependencies

All additions are required methods, no defaults. Existing provider registries/accessors are reused and release their locks before calls.

`bank_writes::ScopeBinding` (implemented here):

```rust
fn observe_enrollment_location(&self, repo: &Path, runtime_root: &Path)
    -> Result<Option<layout_recovery_state::RecoveryPaths>>;
```

This exact signature was agreed directly with the private-state lane; integrate it only once.

`bank_writes::RecoveryFrontier` (future implementer `recovery_frontier`, existing registration `register_recovery_frontier`):

```rust
fn verified_enrollment_hold_at_bank(&self, root: &Path, bank: &Path, runtime_root: &Path)
    -> Result<Option<scope_binding::VerifiedEnrollmentHold>>;
fn observe_enrollment_bank(&self, root: &Path, bank: &Path, runtime_root: &Path)
    -> Result<Value>;
fn observe_source_markers(&self)
    -> Result<Box<dyn scope_binding::SourceMarkerObservation>>;
```

These preserve the baseline internally located `_verified_hold_at_bank`, `_observe_bank` and `_observe_source_markers` semantics. In particular, hold absence must still inspect the private recovery index; removed markers must not imply empty coverage.

`bank_writes::TurnSources` (future implementer `turn_sources`, existing registration `register_turn_sources`):

```rust
fn check_source_compatibility(&self, state_base: &Path) -> Result<()>;
fn source_barrier_held(&self, state_base: &Path) -> Result<bool>;
```

The private-state lane separately appends its agreed `source_barrier` method. Scope does not introduce a duplicate barrier/provider convention.

Existing dependencies retained: repository_config security scanner, bank_contract observer/coverage, bank_writes security audit, transaction observer/frontier/barriers and project_identity's BankWrites/BankObservation adapters; raw recovery additionally needs its LayoutObservation provider. The corresponding concrete security, recovery_frontier, turn_sources, layout recovery observation/quiescence, checkpoint/recovery-transactions and harness modules are not supplied by this immutable base. Missing providers refuse through the existing typed registry accessors; none are faked in tests or runtime.

The historical `leaves-report.md` line 14 names `turn_check::ScopeBinding` as an implementer obligation. There is no `src/turn_check.rs`, module export, or such committed trait in base `2a12544a8e82e1e922f814f8c27540df471cffa0`. Main explicitly confirmed this base is authoritative and directed no guessed/placeholder adapter. The turn-check adapter awaits its concrete committed API/integration, not an invented duplicate scope registry. The actual current implementer obligation, `bank_writes::ScopeBinding`, is implemented in full.

## CLI

`am identity` is registered once through the established helper/table/composition convention:

- `status --repo . [--harness] [--session] [--observe ...]`
- `preview --repo . --action {create,bind,link,rebind,fork} [--project-id] [--authority] --reason`
- `require --repo . [--initialize]`
- `apply --proposal FILE`

Input acquisition uses the common bounded CLI reader. Domain refusals retain the scope-result envelope. Argparse execution and Python helper-path aliases are deliberately not ported.

## Tests and exact obligation status

`tests/t1_scope_binding.rs` contains active local contracts for exact context fields/purpose, complete namespace rewriting including real record/index/tombstone validation, bundle rejection, named receipt codecs, typed capability signatures, no-locator authority, real missing-principal refusal/source preservation, migration-vs-initialization exclusion, and the production adapter's initialization/locator boundary.

Real-process tests create canonical 0700 roots and inject isolated HOME/config/state/Git environments into owned finite-deadline children. They do not change global environment or register fake security/frontier/hold/source providers. The re-exec harness uses `--include-ignored`, so subsequently removing an ignore cannot accidentally skip its child body. Fault tests invoke the real native binary with explicit machine JSON mode.

No obligation is claimed fully qualified before parent verification. The ledger remains untouched. After the first parent build, recommended status is **partial** for the exact 13 IDs in `/tmp/scope-binding-partial.ids`:

| Exact baseline ID suffix (all prefixed `tests/contracts/`) | Rust representation / remaining test qualification |
| --- | --- |
| `test_scope_binding.py::ScopeBindingContract.test_all_public_mediated_writers_refuse_unresolved_identity_and_raw_reads_remain_available` | Real single/multi writer refusal/raw-read/then-resolved write test written, ignored pending real safety providers |
| `test_scope_binding.py::ScopeBindingContract.test_combined_status_readonly_missing_conflicting_and_known_attribution` | Complete developer setup/conflict/status/source-preservation test written, ignored pending security/frontier/project adapters |
| `test_scope_binding.py::ScopeBindingContract.test_create_has_initialization_only_handoff_without_chicken_and_egg` | Complete create/initialization-context/replay test written, ignored pending security/project adapters |
| `test_scope_binding.py::ScopeBindingContract.test_fork_changes_every_namespace_and_preserves_record_provenance_conditions_and_tombstone` | Active complete-map namespace test plus ignored real clone/two-store test; no full integrated claim |
| `test_scope_binding.py::ScopeBindingContract.test_identity_is_pinned_from_preview_and_rechecked_under_the_bank_lock` | Real native pause/selection-change/under-lock refusal test written, ignored pending security/frontier/turn sources/project adapters |
| `test_scope_binding.py::ScopeBindingContract.test_portable_status_uses_selected_repo_and_preserves_whitespace_paths_despite_ambient_git` | Native selected-repo/whitespace/ambient-authority test written, ignored; relocated native payload qualification remains release-owned, not claimed by replacing the copied Python executable assertion |
| `test_scope_binding.py::ScopeBindingContract.test_rebind_previews_bank_change_reads_both_stores_and_preserves_aliases_and_code` | Complete rebind/readback/alias/index/HEAD/code preservation test written, ignored pending real providers |
| `test_scope_binding.py::ScopeBindingContract.test_registry_only_interruption_and_torn_bank_resume_with_original_bundle` | Both original hard-exit stages, original recovery bundle, dead-owner recovery, finish and historical replay refusal written, ignored pending real providers |
| `test_scope_binding.py::ScopeBindingContract.test_stale_developer_registry_bank_and_remote_proposals_never_overwrite` | All four stale-source refusal branches written, ignored pending security/frontier/project adapters |
| `test_project_identity.py::ProjectIdentityContract.test_scope_link_repair_preserves_bank_and_verifies_current_alias` | Real historical alias scope-link and bank preservation written, ignored pending security/frontier/project adapters |
| `test_project_identity.py::ProjectIdentityContract.test_conflicting_historical_aliases_allow_explicit_scope_link` | Real conflicting aliases, explicit link and old-alias preservation written, ignored pending security/frontier/project adapters |
| `test_repository_observation_batch.py::RepositoryObservationBatch.test_config_and_remote_mutation_between_enrollment_entry_exit_are_fresh` | Real entry/exit configuration and remote mutations written, ignored pending enrollment providers; Python monkeypatch-at-require mint interception is not independently recreated/claimed |
| `test_repository_observation_batch.py::RepositoryObservationBatch.test_subdirectory_still_refuses_repository_root_authority` | Real subdirectory enrollment refusal plus bank/index/HEAD preservation written, ignored pending enrollment providers |

The exact 17 IDs in `/tmp/scope-binding-deferred.ids` remain **unported/deferred**, not marked partial by this lane: all 13 `HeldSourceEnrollment` methods plus RepositoryObservationBatch's changed-HEAD/deleted-locator, late indexed/untracked/raw locators, real verified-hold readonly, and no-live-query-child enrollment contracts. Full held fixtures require real recovery observation/frontier/quiescence, source transport, checkpoint/admission and (for hook cases) native harness modules. No fake hold receipt or mock provider was constructed to substitute for those dependencies. Active type-boundary tests are not advertised as the held exact-retry/no-ACK contract. Their complete exact IDs are retained in the handoff file rather than silently narrowed to context shape assertions.

These are test/integration dependency deferrals, not omitted branches in scope_binding's production implementation.

## Deltas and foundation

- Runtime root is explicit for enrollment/recovery location APIs, matching the existing Rust layout_recovery_state contract instead of Python `__file__` discovery.
- Rust types make externally forged enrollment/raw-evidence argument types unrepresentable; there are no serialization hooks to disable at runtime. Synchronous enrollment scopes are explicitly thread-local/non-Send.
- Binding-bundle and pinned private artifact reads use the named shared `MAX_PRIVATE` bound; canonical bank limits remain unchanged. No unbounded reads or new child processes are introduced in the production module.
- Descriptor-relative no-follow metadata/reads and parent rechecks replace Python path-based lstat/read races; unsafe occupants refuse without repair. Existing durable formats and named codecs are unchanged.
- Shared strict UTF-8/JSON depth/surrogate policy and parser/help differences remain the previously declared cutover deltas; malformed dynamic argument shapes receive bounded typed refusals rather than Python exception internals.
- No foundation additions were needed. `SourceTransportBusy` retains its exact original public message and fixed-size Busy/Initializing cause classification, rather than exposing an interpreter exception chain.

## Verification

Parent verification on Ubuntu 26.04, Linux 7.0.11 x86_64, rustc 1.94.1:

| Check | GNU | x86_64 musl |
| --- | --- | --- |
| `cargo fmt --check` | Passed after edit-only reflows | Same source |
| `cargo check --offline --locked --all-targets --features dev-tools` | Passed, zero warnings | All targets compiled in full suite |
| `cargo test --offline --locked --features dev-tools --test t1_scope_binding` (musl adds target) | 9 passed, 13 ignored, three consecutive runs | 9 passed, 13 ignored, three consecutive runs |
| Full suite with `--no-fail-fast --features dev-tools` | Only stale root choice-list assertion failed in `t2_cli`; all other targets passed | Same single failure; all other targets passed |
| Owner-authorized corrected `--test t2_cli` | 14 passed | 14 passed |

The CLI correction derives evolving alternatives from the public declared command surface while preserving exact usage, program, refusal, quoting and exit-2 behavior through the actual executable. No new fixed command list was pinned and no production refusal was weakened. Parent reran only the affected CLI target after this test-only correction, not the broad suites. Direct `am identity --help` also ran successfully, showing status/preview/require/apply. Required panic-site scanning of the two production files found no matches.

Parent marked all 13 authored IDs partial using the documented `am-dev` command. Thirteen concrete safety-provider integration tests remain ignored, and the 17 unwritten/deferred IDs remain unported; those are not passing campaign claims. The absent turn_check adapter remains integration-owner work. No out-of-scope project-identity fix or fake security/frontier/source provider was introduced.

Evidence: `/tmp/agent-memory-scope-binding-proof/` on `omp-beast`. Formatting changes used stdout-only rustfmt and anchored edits, not file-mutating formatter commands. Rust LSP was unavailable (only OmniSharp configured), so investigation used the permitted textual fallback. No macOS/aarch64, native held-source, installed relocation or release qualification is claimed.
