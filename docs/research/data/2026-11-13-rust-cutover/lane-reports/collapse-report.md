# Lane `collapse` report

Worktree: /opt/agent-memory-worktrees/rust-v015-lane-collapse (detached at 5764442c). No Git writes made.

## Method note
`xd://lsp` was unavailable ("No language servers configured for this project"; reload failed). The compiler served as the reference oracle instead: each symbol was deleted, then `cargo check --all-targets` was run until clean, cross-checked with `rg`.

## Seams removed (trait, register fn, global slot, provider accessor, adapter, registration line)
- **records::TombstoneParser**: `records::validate` now calls `tombstones::parse`. `validate_with_parser` and `tombstones::TombstoneParser` are removed.
- **bank_contract::BankObservation**: new pub fns `bank_observation::{scan_hashes, scan_sources}`. Callers use `bank_observation::observe(..).into_contract()`.
- **bank_contract::RouteTable**: removed as dead code. `recover_layout` had no callers, and `am bank recover-layout` routes through `cli/helpers/bank_contract.rs`.
- **project_identity::{BankObservation, BankWrites, LayoutObservation}**: replaced by `bank_observation::markers`, `bank_writes::OPERATIONS` and `layout_recovery_observation::read_source`.
- **bank_transactions::{IndexInvalidation, ProjectIdentity}**: replaced by `recall_catalog::invalidate_after_change`, inlined in `invalidate_optional_indexes`, and by `project_identity::status`.
- **bank_writes::{ScopeBinding, Checkpoint, Security, TurnSources, RecoveryTransactions} and CanonicalWriterBarrier**:
  - Callers now use `scope_binding::{context_value, require, observe_enrollment_location}`. `optional_project` is now `pub(crate)`.
  - Checkpoint calls go to `checkpoint::{validate_write, validate_preparation}`, and security calls to `security::{guarded_start, audit}`.
  - Barrier calls go to `turn_sources::{canonical_writer_barrier, source_barrier, check_source_compatibility, source_barrier_held}`.
  - `CanonicalWriterBarrier` had one implementer and was pure indirection, so `SourceBarrier::finish` is now an inherent method.
  - Recovery calls go to `recovery_transactions::{apply_checkpoint, apply_transaction, nested_receipt, recover_nested}` with `cli::runtime_root()`, which is identical to the deleted local helper.
- **handoff::Checkpoint**: `sections`, `reusable` and `assess` call `checkpoint::{headings, FIELDS, EXTRA, list_text}`. The `*_with` variants are removed.
- **Security duplicates**: `repository_config::SecurityScan`, `sharing_hold_mapping::SecurityScan` and `bank_writes::Security` are replaced by one new function, `security::has_reasons(&Value)`, plus `security::scan` and `security::audit`.
- **turn_check::{ScopeBinding, TurnSources}** and the single-implementer `NoticeStore` trait:
  - `check` calls `scope_binding::private_enrollment_context(repo, &cli::pack_root()?)`, then `.held()` and `recheck_private_enrollment`.
  - `notice_required` calls `turn_sources::with_notice_store`, whose `NoticeStore` is now a concrete struct.
- **bank_state::ComparisonView and discovery::Comparison**: replaced by `retrieval::observed_context`, `comparison::inspect` and `comparison::safe_text`.
- **sharing_recovery::SharingBoundary**: replaced by `sharing_boundary::inspect`.
- **sharing_hold_mapping::{HoldCauses, SemanticSharing, SecurityScan}**: replaced by `sharing_holds::cause_id`, `semantic_sharing::*` and `security::has_reasons`. `memory_publication` calls `semantic_sharing` directly.
- **diagnostics::Projections**: replaced by `projections::{MAX_BYTES, value}`.
- **doctor_fix::Diagnostics**: `doctor_fix::run` calls `diagnostics::run(env, services, ..)` and `diagnostics::Report`. `diagnostics::Runtime` is removed. Production is identical, because the CLI passes `Environment::process()` and `Services::registered()`.
- **private_state::PrivateStateHooks**: replaced by direct calls to `omp_boundary::*`, `admission::read_input` and `installation::{read_receipt, verify_targets}`.
- **layout_restore::LayoutRecovery**: about 60 call sites now call `layout_recovery::*` directly, with signature-exact arguments. The `&dyn LayoutRecovery` parameter of `remove_directory` is removed.
- **turn_sources::PublicationGate**: `turn_sources` and `hooks/boundary_ledger.rs` call `memory_publication::request_value` and `sharing_recovery::replay` directly.
  - `ContractError::missing_source_publication` is removed. It was a dead constructor in `error.rs`; the `Kind` variant is kept, append-only.
- **omp_boundary::{OmpTransport, OmpStream}**:
  - Boundary calls now go to `omp_transport::{complete, list_pending, MAX_PAYLOAD}`.
  - The adapter bodies became `pub(crate)` functions `omp_transport::{boundary_pending_inventory, boundary_all_segments, boundary_source}`.
  - Stream calls go to `omp_stream::drain_state(&Installed::new(rt), ..)` and `omp_stream::main(.., &BoundaryStartup)`, and `InstalledStream` is removed.
- **omp_stream global transport slot**: `Installed::transport()` returns `Transport::installed()`. The `omp_stream::OmpTransport` trait itself is kept (see below).
- **console::InstalledReceipt**: `bootstrap_command(args)` is now `bootstrap_command_with(args, installation::installed_receipt)`. The receipt read is passed as a lazily invoked closure parameter.
- **cli::PackRoot**: `cli::pack_root()` calls `installation::runtime_pack_root()`. A `#[cfg(feature = "dev-tools")] cli::use_development_pack` hook is consulted first (see Tests).
- **security::MigrationAudit**: `security::audit` calls `migration::validate_audit_location`, which checks against `migration::executable_root()`. `LocationAudit` is removed. A `#[cfg(feature = "dev-tools")] migration::use_development_runtime` hook is added.
- **cache_recovery::Installation global slot**: production `observe`, `qualify` and `recheck` use `cache_recovery::Installed`, whose bodies moved from `installation::CacheInstallation`. The trait is kept (see below).
- **managed_source::Installation**: replaced by `installation::managed_source_info` (`pub(crate)`).
- **artifact::MigrationContract global slot**: `inspect` and `inspect_runtime` pass `Some(&migration_contract::Bundled)` directly.
- **pack_validation::CommandSurface global slot** (and its generation counter): `with_validated` uses `cli::surface::Declared::current()`. The validation memo stays, keyed by inventory with a constant generation.
- **wiring.rs**: `cli/wiring.rs` keeps only `LifecyclePrivateState`, because it implements excluded `installation::seams`.
- **Acceptance grep**: `rg 'fn register_' src` now lists only the excluded registrations:
  - admission::Comparison, bank_contract::RecoveryCoverage, bank_writes::RecoveryFrontier, private_state::RecoveryQuiescence and turn_check::MemoryRefresh;
  - the bank_transactions::TransactionValidator registry, installation::seams::* and omp_installation::OmpActivation.

## Edits in excluded files (each only changes a provider call into a direct call, as Main approved)
- `src/purging.rs:1016`: `bank_writes::turn_sources_provider()?.canonical_writer_barrier(..)` became `turn_sources::canonical_writer_barrier(.., None, &cli::pack_root()?)`.
- `src/installation/providers.rs`: the impl blocks of `cli::PackRoot`, `console::InstalledReceipt`, `managed_source::Installation` and `cache_recovery::Installation` were deleted, as Main's rule allows.
  - Their bodies became `pub(crate) fn runtime_pack_root`, `installed_receipt` and `managed_source_info`.
  - The cache body moved to `cache_recovery::Installed`. The module doc was updated.
- `src/installation.rs`: re-export lines only. Removed the deleted struct names and added `pub(crate) use providers::{installed_receipt, managed_source_info, runtime_pack_root}`. This is required for the change above to compile.

## Seams and traits kept, with reasons
- **Excluded by the assignment**:
  - `admission::Comparison`;
  - `bank_writes::RecoveryFrontier` and `bank_contract::RecoveryCoverage` (both recovery_frontier);
  - `private_state::RecoveryQuiescence` and `turn_check::MemoryRefresh`;
  - the `bank_transactions::TransactionValidator` registry;
  - `installation::seams::*`, together with the `LifecyclePrivateState` adapter.
- **omp_installation::OmpActivation and InstallationActivation**: an `installation::Services` composite; lifecycle lane (Main's decision).
- **Genuine parameter abstractions**: no global slot remains for any of these. Each now has a production implementation used directly, plus test or alternate implementations passed explicitly.
  - `artifact::MigrationContract`: an `Option<&dyn ..>` parameter of `inspect_with` and `inspect_runtime_with`. Tests pass a `FixtureCatalog` for fixture packs.
  - `pack_validation::CommandSurface`: a parameter of `validate_with_surface` and `with_validated_surface`, and held by `migration::Runtime`. Test `NoExecutableSurface` and retained runtimes pass their own.
  - `cache_recovery::Installation`: a parameter of `observe_with`, `qualify_with` and `recheck_with`.
    - Test `FixtureInstallation` inspects fixture packs that are not full packs.
    - Test `Writer` injects a concurrent write at a fault point.
  - `omp_stream::OmpTransport`: returned by `StreamAuthority::transport()`. Test `TestTransport` drives frame scenarios.
- **Genuine abstractions named in the assignment**, all kept: `omp_stream` `frame::Responder`, `linux_image::ImageProbe`, `clock_window::NativeImage`, `executor::BusyError`, `migration_contract::Codecs`, `retrieval::Sources`, `scope_binding::SourceMarkerObservation`, `omp_transport::observe::ObserverScope`, `omp_installation::HeldSource` and the `bin/am_dev` release providers.
  - `CanonicalWriterBarrier` was removed: a single implementer, pure indirection.

## Test fakes on removed seams, and what each became
- **Converted to real implementers, with registration lines deleted**:
  - tombstone parser registrations in `t1_bank`, `t1_bank_observation` and `t1_scope_binding`;
  - BankObservation in `t1_bank` and `t1_bank_transactions`;
  - security scan in `t1_bank_transactions`, `t1_layout_recovery_state`, `t1_project_identity`, `t2_lifecycle` and `t2_sharing_holds` (`FixtureLabels`);
  - Diagnostics in `t2_lifecycle`, where `doctor_fix` now uses the passed env and services;
  - IndexInvalidation in `t2_recall_catalog`;
  - in `t1_project_identity`, the fake LayoutObservation and MarkerObservation, plus the `BankMarkers` and `WriterOperations` wrappers that forwarded to the real implementations;
  - MigrationContract in `t2_recall_catalog`.
- **Adapter-method tests now call the real functions directly**:
  - `t1_scope_binding`, `t1_security`, `t1_checkpoint_inactive`, `t2_retrieval` (projections), `t2_publication` (request and replay) and `t2_layout_recovery_observation`;
  - `t1_a2`: the handoff test uses the real checkpoint parser.
- **Tests that asserted a "not registered" state, now asserting the real behaviour**:
  - `t1_a2`: tombstone dispatch.
  - `t1_bank`: three tests.
    - `tombstone_routes_dispatch_to_the_real_parser_even_when_nested`.
    - `write_proposals_accept_unresolved_scope_and_refuse_invalid_paths`.
    - `real_observation_..._refuses_without_frontier_or_writes`: still fails on unregistered `recovery_frontier`, which is an excluded seam.
  - `t1_bank_observation` and `t1_project_identity` (raw identity).
  - `t1_private_state`: `inventory_observation_is_readonly`.
  - `t1_turn_sources`: an absent publication request keeps the source pending.
  - `t2_delivery`: `missing_installation_registration_is_an_explicit_refusal` was removed, because no registration exists any more.
- **Console receipt**: `t2_installation` `bootstrap_rewrites_..` now passes fixture receipt data through `bootstrap_command_with`. The data is a real fixture, not a fake provider.
- **Converted to `#[cfg(feature = "dev-tools")]` hooks**, because a test binary is not an installed runtime and no real fixture exists short of installing the test executable under a mutated process environment:
  - `cli::use_development_pack`, used by `t2_diagnostics` (native install child), `t2_initialization` (child entry) and `t2_sharing` (in-process).
  - `migration::use_development_runtime`, used by `t2_migration` (child).

## pack_root versus runtime_root: baseline check (no parity change)
- **Main's rule**: switch to `cli::runtime_root()`, the pack's grandparent, only where the baseline passes the runtime root.
- **What the baseline passes**: never the grandparent.
  - `turn_check.py:89` calls `scope_binding.private_enrollment_context(repo)`, and `turn_sources.py:249` and `:958` do the same.
  - The runtime used underneath is `layout_recovery_state.py:163`: `runtime = Path(__file__).resolve().parents[2]`. That is `skills/agent-memory`, the pack root, because `__file__` is `skills/agent-memory/scripts/agent_memory/layout_recovery_state.py`.
  - `canonical_writer_barrier(repo, ordinary_bank=None)` (`turn_sources.py:200`) and recall-catalog invalidation reach the runtime through the same `layout_recovery_state` paths.
- **Result**: the existing `cli::pack_root()` matches the baseline. I made **no defect fix** here.

## Behaviour notes
- **Missing-provider refusals** are gone for every collapsed seam, by design. This removes refusals such as `SourcePublicationUnavailable` and "Pack location is not registered".
- **`private_state::repository_inventory`** keeps an unused `_runtime_root` parameter, because its caller `updating.rs` is excluded. This is for the lifecycle lane to remove.

## Verification
Additional test conversions made during the final runs, because these tests asserted states that no longer exist:
- **`t1_bank`**: `real_observation_…_refuses_without_frontier_or_writes` now expects apply to refuse with the receipt-based pack-root guidance ("… pass --pack PATH"). The test binary is not an installed runtime. The test still asserts that nothing was written.
- **`t1_bank_observation`**: `absent_domain_provider_…` now expects the excluded `recovery_frontier` refusal.
- **`t1_bank_transactions`**: the assertion that `proposal_value` refuses on a missing scope provider is removed.
- **`t1_private_state`**: became `native_absence_observation_is_readonly` and `inventory_observation_is_readonly`.
- **`t1_security`**: a migration audit at a missing locator is refused by the real locator check.
- **`t2_layout_core`**: `layout_recovery_seam_refuses_without_a_registered_provider` is removed, because the seam no longer exists.
- **`t2_omp_boundary`**: `missing_transport_and_stream_registrations_are_typed_refusals` is removed, because the seams no longer exist.
- **`t2_lifecycle`, doctor-fix**: became `doctor_fix_on_a_healthy_installation_repairs_nothing`. It runs the real diagnostics, then asserts no repairs and an unchanged world.
- **`t2_lifecycle`, purge**: became `purge_refuses_without_touching_the_bank_outside_an_installed_runtime`.
  - The refusal now comes after the lifecycle lock, whose lock and owner files persist by design.
  - Before, the missing security scan caused the refusal earlier.
  - The test now snapshots the repository, not the whole world.

Commands run in this worktree, all passing (macOS: Darwin 27.3.0 arm64, aarch64-apple-darwin, rustc 1.99.1 (58cf8f9c3 2026-10-13)):
- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: 0 errors and 0 warnings. `cargo check --offline --lib` without dev-tools is also warning-free.
- `cargo test --offline --no-fail-fast --features dev-tools`, the whole crate: exit 0, with 81 `test result: ok` lines and no failures.
- `~/.local/state/agent-memory/research/rust-cutover/linux-run.sh /opt/agent-memory-worktrees/rust-v015-lane-collapse collapse --features dev-tools`: exit 0, "LINUX OK (aarch64-unknown-linux-musl run, x86_64 musl built)".
- `rg 'fn register_' src`, excluding `src/bin`: only the excluded registrations listed above, plus `cli::register_providers`.
- `tests/wiring.rs` passes. `tests/wiring-pending.txt` is unchanged; its three entries are still accurate.
- The `t1_bank` and `t1_private_state` re-runs requested by Main pass, alone and in the full run.
