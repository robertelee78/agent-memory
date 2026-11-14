# Recovery frontier lane report

Worktree `/opt/agent-memory-rust-recovery-frontier`, branch `lane/recovery-frontier`. Parent rebased the authored source onto approved `a725b97b34033221d8927cc7ecb95ddeaf694130` (security/observation/catalog/lifecycle base), reporting authored source commit `54b1b6b`. The worker performed no Git operation; the original implementation base was `681c3b7f3e9cff5506709fef7a9a4fb8e4d5b31c`.

## Delivery boundary

The complete owned domain implementation, assigned providers, approved shared extensions and production composition are authored. The installation owner approved and the lane applied receipt-bound runtime-pair selection and bootstrap-refusal guidance. This is **source ready, not a verified complete lane delivery**: parent checks of the latest batch and the owner's subsequent integration API cutover remain outstanding. No guessed source root, placeholder provider, fallback scanner, synthetic verified hold, panic, or no-op bypasses that boundary. The worker ran no build/test/lint/formatter/smoke command.

Applied files, exact list:

- `src/recovery_frontier.rs` (new).
- `tests/t1_recovery_frontier.rs` (new).
- `src/lib.rs`: one module declaration.
- `src/bank_writes.rs`: required `RecoveryTransactions::recover_nested` method appended; no defaults or existing signature changes.
- `src/bank_state.rs`: append two required ComparisonView methods, rename the existing accessor and migrate its one caller; existing refusal kind/message unchanged.
- `src/project_identity.rs`: approved crate-visible accessor for the existing LayoutObservation registry, with typed absent-provider refusal.
- `src/private_state.rs`: append only the required RecoveryQuiescence::validate method.
- `tests/wiring-pending.txt`: remove the two implemented frontier/coverage pending entries; extend the existing ComparisonView dependency to retrieval/comparison/recall_catalog; append the actual layout_resolution dependency.
- `tests/t1_scope_binding.rs`: enable five existing-bank binding/status/link scenarios and two enrollment scenarios with actual frontier callbacks in the existing isolated helper; add fresh after-body child completion receipts. Five canonical-write/native-write scenarios retain precise real installed-runtime prerequisites.
- `tests/t1_security.rs`: preserve every full evaluator/workflow body; replace stale module-only ignore reasons with the actual installed-v3 native-runtime prerequisite (plus genuinely unported checkpoint/sharing modules where applicable).
- `tests/t1_turn_sources.rs`: enable all four ordinary registration/ACK/wake/cap/publication scenarios with actual frontier callbacks and existing explicit-runtime APIs; require fresh child completion evidence. The full 128-registration/128-ACK capacity journey alone has an explicitly passed 600-second deadline following parent's syscall diagnosis; all other callers explicitly retain 120 seconds.
- `tests/t1_bank.rs`: preserve four full writer bodies; replace stale provider/module ignore lists with the missing real principal/project fixture and receipt-backed barrier prerequisites.
- `tests/t1_bank_transactions.rs`: enable the complete partial-rename/finish/rollback convergence body in merged World isolation with actual frontier callbacks.
- `tests/t2_recall_catalog.rs`: enable both final-content/immutable-reader scenarios in merged World isolation; retain real retrieval/discovery prerequisites for the two query scenarios.
- `tests/t2_lifecycle.rs`: generate the valid legacy MEMORY-only purge fixture using public bank_contract::router_text plus parse_router and all six omissions; enable both full purge journeys, align explicit/process state roots inside owned re-exec Worlds, inject actual frontier only in the test process, bound purge children and require normal completion proof or intentional fault-entry evidence plus exit86 and subsequent recovery.
- `tests/support/mod.rs`: append the owner-approved with_frontier helper using the merged World environment/path/child convention; actual providers/callbacks only, exclusive fresh completion receipt after successful body, parent requires exact receipt and exit0. Owned frontier tests reuse it too.
- `src/installation/providers.rs`: owner-authorized executing_runtime pair API; pack_root and omp_locations delegate to the same single-receipt authority; missing/foreign executable refusals direct to trusted bootstrap.
- `src/installation.rs`: public executing_runtime export.
- `src/cli/mod.rs`: one real Arc<FrontierProvider> cloned into both assigned registries in the existing composition point.
- `tests/t2_installation.rs`: reuse existing installer fixtures for no-receipt, installed development symlink and foreign-executable locator contracts with unchanged filesystem snapshots; opaque fixture bytes do not qualify native CLI installation.
- `tests/t2_cli.rs`: migrate the existing real no-receipt CLI refusal expectation to approved bootstrap guidance, retaining status/stdout/explicit-pack assertions.
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/recovery-frontier-report.md` (this explicitly requested report).

External handoff: `/tmp/recovery-frontier-obligation-ids.txt` is TSV `status`, exact baseline ID, reason. It contains 22 partial and twenty unported entries; parent must not promote an entry merely because source is authored or an ignore removed. The obligation ledger is untouched.

The worker executed no Python, Git mutation, commit/push, sibling import/rebase/cherry-pick, tracker/Vox operation, build/test/lint/formatter/smoke command. Parent performed the explicitly approved rebase. No new crate, foundation edit, unsafe code or production subprocess is introduced.

## Approved shared interfaces and runtime composition

### Reuse existing ComparisonView

`src/bank_state.rs:21-24` is the existing T1-owned `ComparisonView` with `observed_context`. Design line 163 and `obs-report.md` explicitly require frontier/comparison/recall_catalog consumers to extend this one interface. Textual reference inspection found zero implementations in source/tests and one private accessor caller (`inspect_context`, line 336).

Applied required appended methods; future T2a composite implementer delegates to actual comparison and recall_catalog:

```rust
fn invalidate_frontier_transition(&self, repo: &Path) -> Result<String>;
fn safe_text(&self, value: &Value, maximum: usize) -> Result<()>;
```

Renamed existing `fn comparison_view()` to `pub(crate) fn comparison_view_provider()` and migrated its one caller, rather than adding an alias/wrapper. Preserved its existing error kind/message. Frontier uses that accessor. The provider is acquired before invalidation's recoverable-cache fallback, so a missing registration cannot be swallowed as successful invalidation.

### Reuse existing LayoutObservation

`src/project_identity.rs:53-55` owns the exact raw-observation callee interface (`read_source(bank, relative, limit)`). The approved rebase supplies and wires the real layout_recovery_observation implementation. The lane added an accessor immediately after register_layout_observation:

```rust
pub(crate) fn layout_observation_provider() -> Result<Arc<dyn LayoutObservation>>;
```

The accessor clones the existing registry, releases its lock before callbacks, preserves registry-poison refusal and uses existing `ContractError::missing_provider` for absent layout_recovery_observation. No second raw reader/registry or test implementation was added. The actual T2d implementation is wired on the owner's later integration head, not imported into this frozen branch.

### Reuse existing RecoveryQuiescence

Appended to `src/private_state.rs:68-76` existing `RecoveryQuiescence`; zero implementations at this base:

```rust
fn validate(&self, repo: &Path, value: &Value,
    runtime_artifact_digest: Option<&str>, required_routes: &[String],
    publish: bool, owner: Option<&[u8]>, runtime_root: &Path) -> Result<Value>;
```

Future implementer is the same recovery_quiescence module that provides existing `selected_attestation`. Existing accessor and error kind remain unchanged. Frontier already delegates to it; no parallel quiescence registry is introduced.

### Executing runtime root and final CLI wiring

The `FrontierProvider` stores one trusted composition callback `fn() -> Result<(PathBuf, PathBuf)>`, returning executing pack and binary coherently. Implicit-runtime methods resolve that pair exactly once per call. Domain functions continue accepting explicit runtime roots, matching layout_recovery_state and existing private-state/turn-source APIs. Actual hold measurement uses artifact::inspect_runtime(runtime, binary), never a pack-only digest.

The single authority is installation::executing_runtime(env, executable): one trusted rust_receipt read filtered to v3, source_binary from that exact receipt, canonical executable equality, and the receipt source.pack returned with its selected binary. pack_root delegates to this API; omp_locations now gets state through locations and pack/binary together through this API. Production current_exe is only a checked equality argument, never the selected binary or a fallback. Missing receipt or foreign executable directs to the trusted bootstrap installer. Development selects recorded source.path/target/release/am; release selects recorded source.path/bin/am.

The bank_observation -> RecoveryFrontier::observe_owned -> observe -> state::paths -> state::_paths_for_bank call chain still canonicalizes the executing runtime root and excludes recovery state beneath it, even for an empty index/no hold. That trust check is not deferred. The two explicit-root hold adapters (verified_hold_for_runtime and verified_enrollment_hold_at_bank) resolve the authorized pair once, require canonical supplied root equality with its pack before any hold observation, and pass that coherent pair onward. Explicit-only observe_bank/ledger/evidence/validate_quiescence remain independent of the production locator. Scope/source callback fixtures select the same explicit roots their domain calls use.

Both actual registrations are appended at the single cli::register_providers composition point, sharing one Arc<FrontierProvider>:

- `bank_writes::register_recovery_frontier(Some(provider.clone()))`.
- `bank_contract::register_recovery_coverage(Some(provider))`.

The real no-receipt locator scenario asserts bootstrap guidance and retained original bytes. A new real-adapter regression rejects both mismatched explicit hold roots before observation and preserves retained original/marker/head bytes and the empty foreign root. It asserts consumer outcomes, not callback plumbing. The installation regression uses existing real installer fixture operations rather than fabricated receipts or new providers; it proves locator selection only. There is no CARGO_MANIFEST_DIR/source/binary-parent production fallback, direct private receipt read, second registry or startup domain operation.

All currently approved glue is applied. Owner integration will separately move the sole marker constants and append any required trait methods at its new base; the worker must receive that tree before adapting exact new signatures.

## Complete 971-line baseline function/branch map

The full `recovery_frontier.py` was read before implementation, including all reader-truncated ranges. Domain functions have explicit runtime parameters where the Python module inferred its executing pack.

| Baseline | Rust representation and retained behavior |
| --- | --- |
| `VerifiedHold` | Alias to existing non-JSON `scope_binding::VerifiedEnrollmentHold`; minted only after real source/index/ledger/runtime/scope/attestation checks. No caller mapping becomes authority. |
| `_observe_source_markers` | `observe_source_markers`, `SourceMarkerGuard`; thread-local nested state, explicit finish, non-Send guard and Drop restoration on refusal/unwind. |
| `marker_bytes`, `_marker_bytes` | `marker_bytes`, `read_marker`; ordinary 4096-byte disclosure bound, exact owned transaction witness up to MAX_PRIVATE, regular/single-link no-follow descriptor read, before/open/after/named signatures and pinned directory recheck. Source-mode canonical-writer checks preserve Busy/Initializing precedence without marker repair. |
| `marker_value` | Exact HOLD/NESTED/TRANSACTION schemas, UUIDs, allowed transaction kinds, canonical byte equality; unknown/orphan input never permits ordinary use. |
| `ledger`, `_ledger` | `ledger`, `ledger_at_bank`; exact indexed head, generation range, complete immutable predecessor chain within MAX_PRIVATE, original termination, unique units, bounded exact ranges, all states/actions/receipts, complete terminal receipt versus partial receipt, revision child retention. |
| `range_value` | Exact three fields, UUID source, integer-not-bool and 0 <= start < end <= 1 MiB. |
| `evidence` | Immutable manifest/map/current alternate map/proposal/review hashes, complete project observation, duplicate source identities, exact pending original byte counts/hashes/range bounds. Original path derives from source UUID, never caller `blob`. |
| `retained_handoff` | Cycle/absence refusal, exact independently reviewed predecessor marker/frontier/identity, retained prior generation and rolled-back dependency chain. |
| `observe`, `_observe_bank`, `_observe_index` | `observe`, `observe_bank`, `observe_index`; fresh location comparisons, active/interrupted/rolled-back selection, competing operations, exact owned child transaction exception, private-binding/index continuity, marker/sentinel pair, full non-cleaned ledgers/evidence/reference maps, pending-state consistency, journal/nested journal linkage, retained rollback handoff, unresolved hold loss, conflicting projects, ordinary parsed router versus lifecycle-only null identity, exact canonical token digests. |
| `check` | Fresh full observation against canonical object-order-independent expected token, clearing only comparison review_digest. |
| `verified_hold`, `_verified_hold_at_bank` | Fresh scoped location plus full source/index observation, exact typed marker, measured executing runtime capability/digest, ledger/evidence, ordinary real scope or exact raw rolled-back router and `RawIdentityEvidence`, checkout continuity, actual selected independent runtime attestation, final full frontier recheck. Missing providers cannot mint a hold. |
| `rolled_back_predecessor` | No-marker branch checks markers, private binding/sentinel/orphans and every retained ledger/evidence; present-marker branch requires actual verified hold and exact rolled-back state. |
| `same_hold` | Exact complete typed hold comparison, including marker/token/scope/runtime; absent values are not holds. |
| `require_scope` | Initialization-only absent-router exception; otherwise observe even no hold, ordinary scope or full verified held scope, project/expected continuity, owner/transaction/migration options. Adapter maps only Value::Null to baseline project=None, text to explicit project, and rejects every other type before resolution. |
| `pending_rows` | Before/after full check, all active/rolled-back pending units, original-derived exact UTF8 ranges, MAX_FILE indivisibility and real security scanner, comparison-only/unverified labels and complete unit-set equality. |
| `inherited_pending_rows` | Rolled-back original-derived range disclosure, real security, ordinary reader bound and global opaque-unit uniqueness, without relying on invalid canonical router. |
| `review_for_comparison` | Actual pending reader delegation. |
| `require_complete` | Reject incomplete coverage or a surviving hold before refresh/publication. |
| `public_coverage` | Baseline complete/incomplete semantic label, opaque unit IDs/count and exact required action, never private source bodies/ranges/hashes. |
| `invalidate` | Actual existing ComparisonView/recall_catalog callback; required registration acquired first; recoverable callee failure returns baseline deferred-to-live-frontier-check, not fabricated successful invalidation. |
| `install` | Actual owner/marker or exact typed predecessor gate, UUID/operation checks and competing-operation refusal; exclusive immutable initial ledger/head, original fault stage, observed index compare/replace and external binding, invalidation, sentinel value returned for caller's exact atomic handoff. Interruption preserves original/private evidence; no marker is invented internally. |
| `claim_value` | Exact bounded claim fields, source-unit identity, two-fragment/reference limits, contained nonoverlapping ranges, exact record UUID/version/path/hash, opaque refs, duplicate reference refusal, none/partial/full semantics and entire framing-preserving union. |
| `proof_input` | Deep copy, clear only recovery review_digest and canonical proof hash. |
| `validate_comparison_review` | Full token recheck, empty-frontier anti-substitution, exact one retained review, complete field set/IDs/source/map/head/proof/output/claim binding, approved decision, real time/comparison safety and actual reviewing harness/session. |
| `reservation` | Real bank validation/record parsing, all mandatory identity witnesses (including source captures/superseded owners/legacy derived-from), security-bound indivisible pending rows, exact baseline extremal disclosure/proof skeleton with maximum escaping, row/64 KiB bounds and greatest proof witness. No mock comparison proof grants authority. |
| `advance_ledger`, `restore_hold`, `retire`, `resolve` | Delegate through required T1-owned LayoutResolution interface to actual future callee; restore_hold retains byte return type. No mutation provider defaults. |
| `resolve_reference` | Actual active/terminal reference maps, exact one locator, ID/hash/version/path binding, opaque-version semantics and ambiguity refusal. |
| `recover_nested` | Existing `bank_writes::RecoveryTransactions` registry extended with real callee method; no second recovery-transaction registry. |
| `capture_scope` | Advisory unresolved ordinary preview only; capacity and typed missing providers propagate; any marker/active interrupted operation keeps refusal and source evidence. |
| `private_preview` | Actual observed frontier, mandatory protected output for plans/recovery proposals, durable existing write_private API and body-free envelope, unchanged no-plan ordinary return. |
| `validate_quiescence` | Required existing RecoveryQuiescence delegation including measured digest, routes, publish and owner. |
| `verified_child_transaction` | Exact owned marker/journal/child proposal/hash/parent/project/verified receipt/head/manifest/scope checks, actual raw LayoutObservation and RawIdentityEvidence, fresh real raw scope and final marker check. |
| `prospective_token` | Independently bound complete raw manifest, no competing private operation, exact predecessor authority or absent markers, all retained terminal/rollback ledger/evidence/reference pins, fresh operation identity and inherited pending obligations; explicitly capacity-only placeholder digests, never an observed authority. |
| `successor_token` | Capacity-only copy, sorted pending IDs, selected generation increment/hash sizing witness, complete-only hold/sentinel removal, cleared review digest; input unchanged. |

There are no intentionally omitted baseline domain functions or meaningful branches. Unavailable callees above remain actual required interfaces with named refusals; their absent implementations are not silently treated as successful operations. Source review/compilation may still identify defects; no execution proof is asserted.

## Public API and future implementers

Public constants at this base: HOLD, NESTED, TRANSACTION, SENTINEL, TERMINAL, UNIT_FIELDS. Public APIs are the table's functions, typed VerifiedHold, SourceMarkerGuard, FrontierProvider::new(executing_runtime_pair_callback), and LayoutResolution registration. Installation additionally exports executing_runtime(&Environment, &Path) -> installation::Result<(PathBuf, PathBuf)>.

`FrontierProvider` implements **every method** of both assigned existing traits:

- `bank_writes::RecoveryFrontier`: capture_scope, observe, observe_owned, verified_hold, check, require_scope, observe_bank, ledger, evidence, validate_quiescence, verified_hold_for_runtime, verified_enrollment_hold_at_bank, observe_enrollment_bank, observe_source_markers.
- `bank_contract::RecoveryCoverage`: public_coverage.

New actual-callee interface in owned module: `LayoutResolution`, with required advance_ledger, restore_hold, retire, resolve methods and immediately following registry accessor. Future implementer is layout_resolution. `bank_writes::RecoveryTransactions::recover_nested` is a required append to the existing actual-callee interface, future implementer recovery_transactions. No implementation exists at this base, so no existing implementer/caller was left stale.

Reuse actual `repository_config::SecurityScan` for pending/reservation safety and ordinary scope/identity APIs. The approved rebased branch now contains and wires security::Security. No Security/CLASSES duplicate or temporary model_reference::CLASSES dependency was added. Existing bank_writes::Security audits remain owned by their actual writer callees, not replaced by frontier.

No baseline CLI parser belongs to recovery_frontier, and no artificial command/helper route is introduced.

## Tests, ignores and exact obligation accounting

Focused target: t1_recovery_frontier. All 20 declarations are active after the real scanner and three reviewer/runtime regressions were added; these are declaration counts, not passing results. The no-receipt scenario and production installation callee now share the approved bootstrap contract.

Tests reuse merged `tests/support::World` for canonical private roots, isolated full HOME/config/state/Git environment and actual finite-deadline owned re-exec children. Filesystem fixtures are retained canonical baseline state snapshots, not providers or verified-hold capabilities. They exercise real marker files, immutable ledgers/complete predecessor chains, external bindings, source originals, private modes, hardlinks/symlinks, corrupt/deleted index, missing/foreign sentinels, missing holds, review/proposal/original changes, duplicate units/revision loss, partial terminal receipts, exact ranges/framing/references, proof digest behavior, capacity-only successors, empty lifecycle identity, extremal capacity and actual kernel writer-lock contention/nested/unwound source-marker scopes. No external network/credential is required. The only child launcher is World's bounded owned process API; no new child registry or isolation convention exists.

The complete extremal reservation test is now enabled against the real security registration. It preserves its entire domain body and uses cli::register_providers, not a fake scanner. Full baseline public layout/admission qualification remains partial. Successful re-exec helper bodies publish a fresh exclusive 0600 completion receipt only after all assertions return; parent requires exact receipt plus child exit0, so an exact-filter zero-test dispatch cannot be counted as success.

Rebased fixture/dependency trace and exact enablement:

- Enabled seven complete scope bodies: combined status; stale principal/registry/bank/remote refusal; actual native identity status with whitespace/ambient Git; both historical registry-link cases; both explicit-runtime enrollment entry/exit/subdirectory cases. The native identity status operation itself does not request a runtime; only its in-process binding fixture needs frontier injection. No native CLI assertion was replaced with a library assertion.
- Enabled all four ordinary source registration/ACK/backlog/wake/cap/publication bodies. They already pass explicit runtime paths into real source APIs, so only their in-process binding snapshot needed the actual frontier adapter. The missing-publication-gate scenario retains its typed refusal and does not fabricate publication.
- Enabled the full existing transaction-convergence body and both catalog final-content/immutable-reader bodies. Their original assertions are retained; merged support::World owns child environment/root/deadline, and with_frontier injects real callbacks only inside that child.
- Enabled both complete purge bodies after the approved fixture repair. The higher-level render proposal rejects zero contexts; public router_text followed by parse_router represents the genuinely valid legacy MEMORY-only router with all six contexts omitted, without adding context files or weakening production parsing. World records explicit root ownership: new owns its root, reopen is borrowed and never deletes it. Global/explicit lifecycle state roots agree. Each nested purge fault child is an owned bounded 120-second process. Normal completion requires a fresh receipt; deliberate faults require exact stage/token entry evidence plus exit86 and all original confirmation/recovery/repeat/source assertions. No ManuallyDrop/leak or production purge/parser change is used.
- Six security evaluators genuinely still need an installed-v3 receipt owning the actual am security-policy subprocess. Their World::evaluate/good helpers invoke env!(CARGO_BIN_EXE_am); a callback registration in the test process does not propagate there. Parent confirmed fresh real installation is currently blocked by the genuine PrivateState adapter and no existing real installed fixture is available. Full public bodies remain intact and precisely gated, not faked or narrowed. Other security ignores retain checkpoint/sharing or installed canonical-write prerequisites as applicable.
- Five remaining scope tests need actual installed-runtime canonical barriers or native writes/recovery subprocesses. Their complete bodies remain present; the actual TurnSources adapter still resolves cli::pack_root, and frontier fixture callbacks do not replace it.
- Four bank writer tests still lack selected real developer/project fixture setup and receipt-backed canonical barriers; their observer-only WriterWorld cannot be turned into authorized scope by registration alone. Six private-state scenarios still need real PrivateStateHooks; native absence is never inferred from missing producer code. Two catalog query bodies still need actual retrieval/discovery. Workspace and raw nested identity scenarios were already unignored by the rebased upstream work and were not edited again.

The TSV includes all 25 exact obligations from frontier/frontier_closure/frontier_substitution, the large-owned-marker eligibility obligation, and the newly enabled existing consumer IDs (42 entries total). Twenty public campaigns remain explicitly unported, not reduced to local tests or promoted to implementation-only. Twenty-two partial entries include authored local contracts and complete newly enabled bodies awaiting parent proof. Existing held/native/OMP campaigns remain unqualified; this lane claims none as complete.

## Declared deltas and safety

- Runtime locations are explicit typed inputs or one coherent receipt-authorized composition pair, replacing Python __file__ inference. Owner approved installed-v3-receipt-only resolution with bootstrap-directed no-receipt/foreign-executable refusal; uninstalled source fallback is unsupported. Explicit-root hold adapters must canonically match that pair, an intentional Rust receipt-authority delta; ordinary explicit-only observations do not acquire a locator dependency.
- Hold measurement binds the actual Rust binary into the existing artifact digest, per cutover design. Pack-only legacy digest is not accepted as executing-runtime authority.
- Rust types make arbitrary expected-marker types and arbitrary hold subclasses unrepresentable. Malformed Value arguments receive bounded named refusals, not Python tracebacks.
- All observations/writes reuse existing foundation descriptor/trust/clock/codecs. No path-based std::fs trust decisions, new unsafe, unbounded reads, process cancellation or deadline changes are added.
- Marker helper accepts only the two actual internal marker names, preventing arbitrary relative-path disclosure; all baseline callers use these names.
- Pending disclosure enforces MAX_FILE before extending a body buffer, rather than first building an oversized concatenation. Exact original range/hash checks remain mandatory.
- Canonical Value equality uses developer_identity::encoded bytes, preserving Python dictionary equality regardless of insertion order. Durable records use the existing sorted/indent2/UTF8/finite/newline codec, with the already-declared shared strict decoding/surrogate/depth deltas.
- No existing permission repair, overwritten source, no-op safety operation, missing-module suppression, or security class duplication is introduced. Recovery evidence remains retained when downstream dependencies refuse after durable intent/index publication.

## Verification handoff

Observed implementation work used only source/document reads, textual references, required anchored edits/new writes, and one jq extraction of exact ledger IDs. Rust LSP is unavailable under the assignment; the allowed textual fallback was used. No build/test/linter/formatter/smoke/Python/Git validation was run, and no GNU/musl/macOS/aarch64 qualification is claimed.

After owner-approved interfaces/runtime wiring are applied and all lanes settle, planned parent commands (not executed here):

1. `cargo fmt --check`.
2. `cargo check --offline --locked --all-targets --features dev-tools`.
3. `cargo test --offline --locked --features dev-tools --test t1_recovery_frontier` three times on GNU, then the same with `--target x86_64-unknown-linux-musl` three times.
4. Focused affected consumers: t1_bank, t1_bank_observation, t1_bank_transactions, t1_scope_binding, t1_private_state, t1_security, t1_turn_sources, t2_recall_catalog, t2_lifecycle, t2_installation, t2_cli, t_support_world, and wiring, with only genuinely satisfied ignores lifted. Native security/real canonical-write tests retain explicit installed-runtime gates and must not count as passes.
5. Full `cargo test --offline --locked --no-fail-fast --features dev-tools` per parent integration policy, GNU and musl; independent Opus source/behavior acceptance.
6. Required source panic/unsafe scan over `src/recovery_frontier.rs`; no scan result is fabricated here.
7. Parent extracts only TSV partial rows' exact IDs and runs `am-dev obligations mark --status partial --ids-file ...`; twenty unported rows remain unported. Actual proof and any stronger status belong to parent.

## Owner integration cutover notes

- The current four public marker schema definitions are ONLY `recovery_frontier::{HOLD,NESTED,TRANSACTION,SENTINEL}`. Owner requested their sole definition move to `bank_writes` at integration. Move the definitions and migrate every source/test import/use in one cutover; do not duplicate values or retain a re-export shim. This frozen lane does not independently introduce a second definition.
- Every requested baseline function is public, including marker_bytes, marker_value, invalidate, range_value, verified_hold, pending_rows, successor_token, reservation, claim_value, resolve_reference and validate_comparison_review. The owned require_scope accepts actual RequireOptions including owned_lock and owned_transaction, so the owner's require_scope_owned trait method can delegate directly without a second implementation. Explicit runtime/binary arguments remain mandatory.
- The public domain verified_hold returns Result<Option<VerifiedHold>>; typed holds expose root(), bank(), token(), marker(), scope(), operation_id() and runtime_artifact_digest(). Existing RecoveryFrontier::verified_hold returns marker bytes to bank_observation. Owner selected verified_hold_value as the distinct future full-context trait method and require_scope_owned for owned scope; apply their exact declarations/delegations only after parent rebases to that owner tree. No guessed overload, re-export shim or authority mapping is added at a725.
- LayoutResolution delegation remains one exact owned callee interface for advance_ledger/restore_hold/retire/resolve; selected_attestation/validate reuse the one private_state::RecoveryQuiescence interface. Owner supplies actual integration implementations/wiring, not fake success providers.
- The historical a725 installation gap is resolved by the owner-approved executing_runtime API, not by console::InstalledReceipt JSON-shape trust or two separately read records. The API retains existing receipt trust/v3/current-executable checks and source_binary selection; pack_root and omp_locations reuse it. Development and release layouts remain distinct, as actually recorded.
- Production composition now shares one real provider Arc across both traits. Test callbacks are only injected in isolated test processes; actual am child binaries retain production receipt authority. The original security native CLI gates therefore remain genuine, not treated as solved by this registration.

## Parent compile and formatting handoff

Parent reported the all-target check compiles; its only warnings were the intentional shared-support re-exports. Parent added the narrow allow(unused_imports) attribute on that pub use, which is preserved. The worker applied the exact formatting changes supplied in `/tmp/agent-memory-recovery-frontier-proof/initial-fmt.log` through anchored edit operations in the eight named source/test files, without running a formatter, build or test and without changing reviewed logic. Duplicate shared-module diff entries were applied once. Final formatting verification remains parent-owned; this note does not claim a rerun.

## Independent-review corrections

The reviewer identified two actual defects; both are corrected without changing public APIs, installation ownership or production wiring:

- `install` now acquires the actual ComparisonView provider before its first durable write and retains that exact Arc for final invalidation. A private `invalidate_with` helper shares the existing invalidation result/error policy with public invalidate; no second registry lookup can lose the preflight dependency. Missing registration therefore leaves the original operation evidence, canonical bytes, index and binding untouched rather than publishing an active operation with no hold. The new isolated regression uses a genuine owned recovery lock, retained source/proposal/map/original files and no substitute comparison provider; it asserts the named refusal and absence of every new ledger/head/index/binding/marker.
- `resolve_reference` now validates actual record-version positivity using the validated decimal Integer text, not as_i64/as_u64. Arbitrarily large positive versions remain accepted; zero, negative integers, floats and booleans refuse. A regression uses the existing retained-evidence fixture and a version above u64, checks exact resolved reference bytes, exercises all four invalid classes, restores the large valid version and verifies original source/hold bytes remain unchanged.

Exact reviewer correction files: src/recovery_frontier.rs, tests/t1_recovery_frontier.rs and this report. Worker ran no checks after those corrections. The runtime API/composition blocker described at that historical boundary is now resolved by the separately authorized batch below.

## First focused proof and fixture corrections

Parent reported post-review fmt/check success, then the first focused run on GNU and musl each produced 7 passes and 12 failures. The worker read the GNU failure log and did not rerun either target. Eleven failures occurred before child launch: with_frontier incorrectly passed its legitimate test-control variable through RunOptions.env, whose existing contract accepts only fault-injection overrides. The helper now inserts that control into its owned World.env and invokes World's ordinary run method; canonical-root validation, private child environment, finite deadline/capture and exact post-body completion receipt remain unchanged. No production allowlist or ambient environment is changed.

The remaining Busy assertion exposed malformed fixture ownership, not a port taxonomy defect. Baseline local_registry and the Rust probe both require operation names prefixed bank- before checking actual lock contention. The test created operation replace, correctly yielding an unknown-owner Contract refusal before Busy. Its real held lock now records bank-replace, preserving the original typed Busy assertion, nested/unwind restoration and source-preservation assertions. The new missing-comparison install regression likewise records the canonical bank-recover name. No production error kind or precedence changed.

Exact first-proof correction files: tests/support/mod.rs, tests/t1_recovery_frontier.rs and this report. No public API/obligation status change, test-body narrowing or worker check occurred. The then-pending bootstrap API/composition is now applied in the authorized batch below.

## Authorized post-full-suite source batch

Parent reported frozen-a725 full-suite summaries: GNU 865 passed/51 failed/33 ignored; musl 907 passed/9 failed/33 ignored. These are parent observations of older bytes, not proof of this batch. Parent ran a separate syscall diagnostic of the already compiled capacity case: substantial active Git/filesystem work, 183 flock calls totaling 0.000740 seconds and no blocked lock. It authorized only the unchanged full-capacity scenario's 600-second bound; the helper now accepts an explicit seconds argument and all other callers retain 120. No cap, ACK count, retry or assertion was reduced.

The scope enrollment fixture now explicitly binds its first independent checkout and links the second to the same established project; all fresh configuration/remote entry/exit refusal and unchanged-byte assertions remain. The prior second bind correctly refused and was not a production defect.

The four reported lifecycle failures all reached the invalid zero-context render fixture (including the fault worker's exit101). The fixture now uses existing public router_text/parse_router, which permit zero routes with all six explicit omissions, preserving MEMORY-only state. Fault stages, entered evidence, expected exit86, exact confirmation reuse and recovery assertions remain unchanged. Latest runtime/fixture/deadline source requires fresh parent focused checks; none were executed by the worker.

Exact additional files beyond the original seventeen are src/installation/providers.rs, src/installation.rs, src/cli/mod.rs, tests/t2_installation.rs and tests/t2_cli.rs (22 total). Obligation TSV remains 22 partial/20 unported; parent owns ledger marking and actual GNU/musl proof.

