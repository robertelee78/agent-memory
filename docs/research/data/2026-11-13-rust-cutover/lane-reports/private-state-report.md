# Private-state lane report

## Delivery and exact files

Worktree `/opt/agent-memory-rust-private`, branch `lane/private-state`, base `2a12544a8e82e1e922f814f8c27540df471cffa0`.

Created:
- `src/private_state.rs`
- `tests/t1_private_state.rs`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/private-state-report.md`

Shared changes:
- `src/lib.rs`: exactly one `pub mod private_state;` declaration.
- `src/bank_writes.rs`: required methods appended to existing `ScopeBinding`, `RecoveryFrontier`, and `TurnSources` traits, detailed below. Existing signatures, registrations and accessors are unchanged.
- `tests/wiring-pending.txt`: new authorized append-only shared registry file, containing only `private_state::register_private_state_hooks waits for installation, omp_boundary, admission` and `private_state::register_recovery_quiescence waits for recovery_quiescence`; parent unions these declarations at integration.
- `tests/obligations/python-baseline-v1.json`: parent used `am-dev obligations mark --status partial` for all seven authored annotations; none is marked ported.

No other source files changed. No CLI/table change, error variant, crate, Cargo manifest or foundation primitive. The implementation worker performed no ledger edit, commit or push; parent owns those delivery operations. Temporary handoff files are `/tmp/private-state-{ported,partial,deferred}-ids.txt`.

## Implementation coverage

Every baseline `private_state.py` function and meaningful branch is implemented; there is no intentionally omitted runtime branch. This is implementation completion, not a claim that missing production dependencies have landed or that checks passed.

- `compatibility`: exact three-token allowlist; duplicate, empty, unknown and mistyped declarations refuse. Absent target has no capabilities.
- `missing_path`: held no-follow directory descriptors distinguish a genuinely missing path from links and inaccessible/non-directory ancestors.
- `repository_inventory`: unions the explicit current repository, available registered checkout/worktree discovery, active/interrupted layout bindings, saved required repositories and bank bindings, and protected native registry/source-state bindings. Missing historical paths remain sorted pinned gaps; explicitly required/private roots are never removed as historical gaps. Final complete bank inventory bound is 1024. Project/layout/native registry digests remain evidence pins.
- `observation_barrier` and `native_absence`: existing-only exclusive source barriers for read-only inspection; otherwise a read-only advisory registry/absence pin. Both before/after lock checks and all native/shared source evidence checks remain. No observation enrolls a missing lock, creates a router, rebuilds private evidence or interprets an unavailable native producer as capable. Ordinary returned errors still recheck absence; unwinding skips post-body rechecks and releases owned resources.
- `required_private_state_capabilities`: stable sorted barrier acquisition, per-bank refusal-to-unknown mapping, separate outer source-synchronization unknown mapping, active-inventory fencing, direct bank_writes location, native producer union, complete retained-layout ledger selection, active-over-rolled-back preference, exact renewed-attestation/routes checks, and final frontier/location/native-pin re-observation. Retained v2 receipt-reader compatibility, selected already-active reader receipt bytes/digest/targets, and uninstall's actually-preserved-reader rule remain enforced.
- `require_compatible`: pending refusal and exact complete-proof recheck, with Python-equivalent order-independent object equality.
- `activation_guard`: sorted exclusive source barriers retained across transition/body/readback; synchronous nested calls reuse the exact enclosing bank set; an unwind-safe restoration guard restores thread-local context. Body refusal/unwind does not perform a success-only final compatibility check.
- `public_summary`: only decision, capability lists and bank/unknown counts; never private locations, source/recovery proofs or digests.
- `prepare_lifecycle`: preserved uninstall reader versus ordinary source selection, at most 1024 explicit protected renewal inputs read at 65536 bytes, exact attestation/review envelope, independent validator invocation, conflicting-operation refusal, exact changed invocation-route selection, check-only native overrides, and publish only outside check mode. Input plans and selected source objects remain unchanged.

Native producer absence is not a supported/capable fallback: an absent required registration is a typed contract refusal. The baseline's optional import branch is replaced with that required cutover registration contract.

## Public API

Constants: `TOKENS`, `MAX_REPOSITORIES`, `MAX_RENEWAL_BYTES`.

- `compatibility(target: Option<&Value>) -> Result<BTreeSet<String>>`
- `missing_path(path: &Path) -> Result<bool>`
- `RepositoryInventory { roots: BTreeSet<PathBuf>, unknown: Vec<Value>, pins: Value }`
- `repository_inventory(installation, current_repo, runtime_root) -> Result<RepositoryInventory>`
- `native_absence(root, base, runtime_root) -> Result<String>`: advisory only, not held-source authority.
- `observation_barrier(root, runtime_root, body: FnOnce(Option<&str>) -> Result<T>) -> Result<T>`
- `required_private_state_capabilities(installation, target_artifact, operation, current_repo, runtime_root) -> Result<Value>`
- `require_compatible(installation, target_artifact, operation, expected, current_repo, runtime_root) -> Result<Value>`
- `activation_guard(installation, target_artifact, operation, current_repo, runtime_root, transition: Option<&mut dyn FnMut() -> Result<()>>, body: FnOnce(&Value) -> Result<T>) -> Result<T>`
- `public_summary(value) -> Result<Value>`
- `prepare_lifecycle(plan, check, runtime_root) -> Result<Option<Value>>`
- `register_private_state_hooks(Option<Arc<dyn PrivateStateHooks>>)`
- `register_recovery_quiescence(Option<Arc<dyn RecoveryQuiescence>>)`

Installation/target/expected parameters above are `Option<&Value>`; operation is `&str`; repository/runtime paths are `&Path`. Every layout-state location/index call passes the explicit executing `runtime_root`, including all newly introduced callbacks. There is no current-directory, source-file or pack-location runtime-root inference.

## Registration and future implementers

Exactly one design-named `PrivateStateHooks` registry covers the upward installation/OMP/admission imports. Its production implementation must compose the real installation (T2e), omp_boundary (T2f) and admission (T2b) functions; no test implementation or fake runtime provider is provided.

Required methods:

```rust
fn native_registry(&self, runtime_root: &Path) -> Result<Value>;
fn native_source_state_bindings(&self, installation: Option<&Value>,
    registered: &Value, runtime_root: &Path) -> Result<BTreeSet<PathBuf>>;
fn has_native_source_state(&self, base: &Path, runtime_root: &Path) -> Result<bool>;
fn shared_native_source_evidence(&self, repo: &Path, base: &Path,
    runtime_root: &Path) -> Result<bool>;
fn required_source_capabilities(&self, repo: &Path,
    runtime_root: &Path) -> Result<BTreeSet<String>>;
fn read_receipt(&self, state: &Path, runtime_root: &Path) -> Result<Option<Value>>;
fn verify_targets(&self, targets: &Value, source: &Value,
    runtime_root: &Path) -> Result<()>;
fn read_input(&self, path: &Path, maximum: usize,
    runtime_root: &Path) -> Result<Value>;
```

The native methods preserve omp_boundary's protected `_registry`, `_source_state_bindings`, `_has_native_source_state`, `_shared_native_source_evidence` and `required_source_capabilities` contracts. `read_input` is admission's protected-file reader, not CLI's permissive caller-input loader. Receipt/target methods preserve installation's active-receipt and exact ownership verification.

`RecoveryQuiescence` is implemented by recovery_quiescence:

```rust
fn selected_attestation(&self, directory: &Path, proposal: &Value,
    runtime_digest: &str, runtime_root: &Path) -> Result<Option<Value>>;
```

Both new registries have the required immediately following crate-visible accessors `private_state_hooks_provider` and `recovery_quiescence_provider`. Registry locks are released before callbacks.

Reused existing bank_writes registries/accessors, with these required append-only methods:

```rust
// ScopeBinding: implement in scope_binding.
fn observe_enrollment_location(&self, repo: &Path, runtime_root: &Path)
    -> Result<Option<layout_recovery_state::RecoveryPaths>>;
// TurnSources: implement in turn_sources; guard releases on Drop.
fn source_barrier(&self, repo: &Path, exclusive: bool, existing_only: bool,
    runtime_root: &Path) -> Result<Box<dyn CanonicalWriterBarrier>>;
// RecoveryFrontier: implement in recovery_frontier.
fn observe_bank(&self, repo: &Path, bank: &Path, owner: Option<&[u8]>,
    transaction: Option<&[u8]>, require_identity: bool,
    runtime_root: &Path) -> Result<Value>;
fn ledger(&self, repo: &Path, operation_id: &str, row: &Value, bank: &Path,
    runtime_root: &Path) -> Result<(PathBuf, Value, Value)>;
fn evidence(&self, directory: &Path, current: &Value, row: &Value,
    runtime_root: &Path) -> Result<(Value, Value)>;
fn validate_quiescence(&self, repo: &Path, value: &Value,
    runtime_artifact_digest: Option<&str>, required_routes: &[String],
    publish: bool, runtime_root: &Path) -> Result<Value>;
fn verified_hold_for_runtime(&self, repo: &Path, owner: Option<&[u8]>,
    runtime_root: &Path) -> Result<Vec<u8>>;
```

`ledger` returns operation directory, ledger head and current ledger; `evidence` returns manifest and reviewed proposal; held-runtime verification returns marker bytes, matching the preexisting trait's held-marker contract. The explicit-runtime hold method is appended rather than changing existing callers' signatures. No defaults or second callee registries are introduced.

The scope method was agreed directly with `PortScopeBinding`, which confirmed implementing the exact adapter. Its independently appended TurnSources `check_source_compatibility`/`source_barrier_held` methods can coexist with this lane's source_barrier method. Parent merges those shared trait additions. No available base implementation of RecoveryFrontier/TurnSources requires an adapter edit in this worktree.

## CLI and foundation

No baseline argparse/CLI surface exists; no CLI handler or table registration added. Future lifecycle callers invoke the typed APIs and serialize `public_summary`, never the private proof.

No foundation additions required. Filesystem inspection uses `fs::DirectoryChain`/descriptor no-follow stat/rechecks; layout evidence reads use bounded `layout_recovery_state`; JSON digests use existing `developer_identity::encoded`; discovery Git uses the existing finite ten-second, bounded, sanitized owned-child `identity_git` implementation. No direct production std::fs or raw libc access was introduced.

## Tests and exact obligation accounting

Authored target contains 10 active tests and 7 explicitly ignored real-dependency tests. Parent reports 10 passed and 7 ignored on each of three focused GNU runs and three focused musl runs. Active coverage: exact declarations, missing-versus-link/non-directory paths, content-free public summary bytes, missing native producer refusal/no writes, present native barrier preservation, protected renewal input/reader rules, exact changed route inventory and source selection, strict envelope keys, missing real renewal validator, and isolated-process read-only missing-registration inventory.

Ignored tests contain actual filesystem assertions, not stubs or fake providers: routerless/ordinary-receipt observation, native normal/error absence races with retained inode/mode/bytes, preservation of the original panic payload and raced evidence, unchanged absence and original ordinary refusal, saved missing bank/repository activation refusal, nested activation/unwind restoration, and lockless native/malformed shared-source detection. They call only the real CLI composition root; dependencies must be implemented and registered there before removing ignores. They use canonical 0700 roots, isolated child HOME/XDG/Git configuration, finite owned-child deadlines, and no global environment mutation, network or user configuration.

Fully ported baseline obligations: **none claimed**. `/tmp/private-state-ported-ids.txt` is empty. This prevents claiming that helper coverage replaces multi-domain lifecycle campaigns.

Partially ported (seven exact annotated IDs, in `/tmp/private-state-partial-ids.txt`): one active partial obligation and six written dependency-blocked obligations. The six ignored annotations require ledger status `partial`, not `unported`, without claiming they executed or completed their baseline obligations.

- `tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_layout_first_native_activation_requires_reviewed_new_routes_without_omp_preseed`: active `lifecycle_selects_exact_changed_invocation_routes_without_publishing` covers the exact six changed invocation kinds, excludes source/unchanged/removed targets, preserves supplied reader-route order/duplicates, selects the preserved uninstall reader, leaves source input unchanged, and keeps check-only overrides out of apply. Full real layout hold, renewed independent review, native enrollment/seal, mixed union, admission and drain/removal campaign is deferred to real recovery/admission/OMP/lifecycle providers.
- `tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_routerless_notes_and_ordinary_receipts_remain_readonly_without_native_obligations` — written/ignored; hooks, scope_binding, recovery_frontier, security registration.
- `tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_lockless_orphan_native_or_malformed_sources_refuse_readonly` — written/ignored native-name/malformed-shared observation; hooks/scope/security; complete released lifecycle preview variants additionally need installation/OMP/adoption fixtures.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_rechecks_normal_and_ordinary_failures` — written/ignored; real hooks/scope/security.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_native_absence_preserves_interrupts_and_fatal_failures` — written/ignored original Rust unwind identity/evidence; real hooks/scope/security.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_unchanged_absence_allows_normal_exit_and_preserves_ordinary_error` — native subcase written/ignored; complete transport/completion families additionally belong to omp_boundary/turn_sources.
- `tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_saved_bank_binding_keeps_missing_repository_and_bank_required` — written/ignored; real hooks/scope/frontier/security.

Deferred unwritten exact IDs (18 IDs in `/tmp/private-state-deferred-ids.txt`; remain unported, never ported):

- `tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_released_v014_sources_preview_then_upgrade_without_inventing_native_authority` — real published-runtime adoption qualification and installation/updating/OMP; no Python-era runtime is executed here.
- `tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_native_barrier_appearing_during_layout_observation_refuses_without_erasing_it` — real frontier/native providers and deterministic producer-boundary injection.
- `tests/contracts/test_private_lifecycle_observation.py::PrivateLifecycleObservation.test_existing_barrier_deleted_before_open_is_not_recreated` — real turn_sources existing-only open boundary; no fake lock provider.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_compatibility_preserves_interrupts_and_fatal_failures` — real frontier/native source callbacks; no fake producer.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_compatibility_race_remains_pending_for_normal_and_ordinary_failure` — real frontier/native boundary injection.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_rechecks_normal_and_ordinary_failures` — omp_boundary transport guard.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_transport_absence_preserves_interrupts_and_fatal_failures` — omp_boundary transport guard.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_completion_absence_rechecks_normal_and_ordinary_failures` — turn_sources completion guard.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_completion_absence_preserves_interrupts_and_fatal_failures` — turn_sources completion guard.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_native_drain_preserves_interrupts_and_releases_source_barrier` — omp_boundary drain and turn_sources.
- `tests/contracts/test_private_observer_interrupts.py::PrivateObserverInterrupts.test_public_source_inspection_preserves_interrupts_with_raced_completion_lock` — turn_sources inspection/completion.
- `tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_omp_first_mixed_pending_rejects_actual_published_old_managed_candidate` — real recovery/OMP/admission/lifecycle and external historical qualification.
- `tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_completed_omp_drains_before_layout_then_old_reader_after_both` — real recovery/OMP/admission/lifecycle and external historical qualification.
- `tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_private_lifecycle_observation_refuses_config_bank_hold_and_attestation_races` — real recovery_frontier/recovery_quiescence and retained evidence fixtures.
- `tests/contracts/test_mixed_private_capabilities.py::MixedPrivateCapabilities.test_managed_update_accepts_repository_under_symlinked_ancestor` — installation/updating/OMP public canonicalization.
- `tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_deleted_registered_worktree_does_not_block_update_repeat_or_recovery` — real installation/updating/recovery/registry fixtures.
- `tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_missing_explicit_repository_remains_an_error_and_preserves_targets` — lifecycle caller's explicit repository gate.
- `tests/contracts/test_update_discovery_gaps.py::UpdateDiscoveryGapsContract.test_available_repository_without_a_bank_stays_in_compatibility_rechecks` — lifecycle caller's observed inventory/readback.

These are test-integration deferrals, not omitted private_state implementation branches. This lane leaves the ledger untouched; parent marks all seven authored annotation IDs partial, including the six written/ignored dependency cases. The remaining 18 unwritten deferred IDs remain unported. The seventh ignored test, nested activation restoration, has no baseline obligation annotation.

## Consumer-visible intentional Rust deltas

- Current repository and executing runtime root are explicit typed inputs, not process cwd/`__file__` guesses. Synchronous closure guards replace Python context managers; thread-local nesting cannot cross async suspension because these APIs are synchronous.
- Cancellation/fatal Rust unwinds are never caught or converted to pending. Allocation aborts remain fatal. Ordinary `ContractError` results use the baseline content-free unknown mappings; Rust does not emulate Python exception classes or traceback/context formatting.
- Missing module registrations fail explicitly, including native inventory/absence. There is no optional-producer capable fallback.
- All trust-relevant ancestors are descriptor-checked/no-follow. A linked ancestor encountered directly as a saved discovery path refuses instead of traversing it. Public lifecycle callers should pass their foundation-canonicalized explicit roots (the mixed symlinked-ancestor public scenario remains caller-owned).
- Malformed dynamic values receive typed refusals rather than interpreter AttributeError/TypeError/KeyError text. Unsupported UTF-8/JSON encoding behavior inherits the existing strict codec. No durable private record format or bytes are changed; observations do not re-encode source records.
- Repository/barrier ordering and emitted capability/pin sets are deterministic.

## Verification and next checks

Implementation checks were deferred to parent. Language-server status found only OmniSharp, so source investigation used the permitted textual fallback. Formatting was captured with stdout-only rustfmt and applied through anchored edits; no file-mutating formatter or Python ran.

Parent verification on Ubuntu 26.04, Linux 7.0.11 x86_64, rustc 1.94.1:

| Check | GNU | x86_64 musl |
| --- | --- | --- |
| `cargo fmt --check` | Passed | Same source |
| `cargo check --offline --locked --all-targets --features dev-tools` | Passed, zero warnings | All test targets compiled in full run |
| `cargo test --offline --locked --features dev-tools --test t1_private_state` (musl adds `--target x86_64-unknown-linux-musl`) | 10 passed, 7 ignored, three consecutive runs | 10 passed, 7 ignored, three consecutive runs |
| Full suite with `--no-fail-fast --features dev-tools` | 480 passed, 1 ledger failure, 9 ignored | 480 passed, 1 ledger failure, 9 ignored |
| Corrected `--test obligations_ledger` after marking authored annotations partial | 5 passed | 5 passed |

The initial full-suite failure was exactly six written/ignored annotations still marked unported. Parent corrected only their ledger status to partial and reran the affected ledger target on each platform, not the full suites. Every other full-suite target passed. Seven runtime dependency ignores remain explicit; no provider, assertion or ignore was weakened. The required panic scan found only `&[][..]`, a fixed empty literal rather than an input-derived slice.

Evidence logs: `/tmp/agent-memory-private-state-proof/` on `omp-beast`. No native-provider/lifecycle campaign, macOS or aarch64 execution is claimed. Those integration qualifications remain with their actual module owners.
