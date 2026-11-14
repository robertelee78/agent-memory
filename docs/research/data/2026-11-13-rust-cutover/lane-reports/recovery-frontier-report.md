# Recovery frontier lane report

Worktree `/opt/agent-memory-rust-recovery-frontier`, branch `lane/recovery-frontier`, frozen base `681c3b7f3e9cff5506709fef7a9a4fb8e4d5b31c`.

## Delivery boundary

The owned domain implementation and consumer tests are authored, and all three requested shared interface edits have explicit owner approval and are applied. This remains a **source-review boundary, not a complete/verified lane delivery**: final production executing-binary selection and bootstrap-refusal guidance require the installation owner's authorized callee change. The owner approved the existing installed-v3-receipt-only runtime-root contract: no receipt means a refusal directing the user to the trusted bootstrap, not an uninstalled source-root fallback. No guessed source root, placeholder provider, fallback scanner, synthetic verified hold, panic, or no-op bypasses that boundary. No build/test/lint/formatter/smoke command has run.

Applied files, exact list:

- `src/recovery_frontier.rs` (new).
- `tests/t1_recovery_frontier.rs` (new).
- `src/lib.rs`: one module declaration.
- `src/bank_writes.rs`: required `RecoveryTransactions::recover_nested` method appended; no defaults or existing signature changes.
- `src/bank_state.rs`: append two required ComparisonView methods, rename the existing accessor and migrate its one caller; existing refusal kind/message unchanged.
- `src/project_identity.rs`: approved crate-visible accessor for the existing LayoutObservation registry, with typed absent-provider refusal.
- `src/private_state.rs`: append only the required RecoveryQuiescence::validate method.
- `tests/wiring-pending.txt`: remove the two implemented frontier/coverage pending entries; extend the existing ComparisonView dependency to retrieval/comparison/recall_catalog; append the actual layout_resolution dependency.
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/recovery-frontier-report.md` (this explicitly requested report).

External handoff: `/tmp/recovery-frontier-obligation-ids.txt` is TSV `status`, exact baseline ID, reason. It contains six partial and twenty unported entries; parent must not mark the unported entries partial merely because implementation exists. The obligation ledger is untouched.

No Python execution, Git mutation, commit/push, sibling import/rebase/cherry-pick, tracker/Vox operation, new crate, foundation edit, unsafe code, or production subprocess is introduced.

## Approved shared interfaces and remaining installation glue

### Reuse existing ComparisonView

`src/bank_state.rs:21-24` is the existing T1-owned `ComparisonView` with `observed_context`. Design line 163 and `obs-report.md` explicitly require frontier/comparison/recall_catalog consumers to extend this one interface. Textual reference inspection found zero implementations in source/tests and one private accessor caller (`inspect_context`, line 336).

Applied required appended methods; future T2a composite implementer delegates to actual comparison and recall_catalog:

```rust
fn invalidate_frontier_transition(&self, repo: &Path) -> Result<String>;
fn safe_text(&self, value: &Value, maximum: usize) -> Result<()>;
```

Renamed existing `fn comparison_view()` to `pub(crate) fn comparison_view_provider()` and migrated its one caller, rather than adding an alias/wrapper. Preserved its existing error kind/message. Frontier uses that accessor. The provider is acquired before invalidation's recoverable-cache fallback, so a missing registration cannot be swallowed as successful invalidation.

### Reuse existing LayoutObservation

`src/project_identity.rs:53-55` already owns the exact raw-observation callee interface (`read_source(bank, relative, limit)`). Zero production implementations and one test implementation exist at this base. Added immediately after `register_layout_observation`:

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

The `FrontierProvider` stores trusted composition callbacks `fn() -> Result<PathBuf>` for runtime root and executing binary. Its domain functions accept explicit runtime roots, matching layout_recovery_state and the existing private-state/turn-source APIs. Actual hold measurement uses `artifact::inspect_runtime(runtime, binary)`, not the pack-only digest.

The approved single locator remains `cli::pack_root` -> `installation::RuntimePackRoot`, requiring an installed verified v3 receipt. No receipt must direct the user to the trusted bootstrap; development install uses a prebuilt executable through install.sh --development-source. No CARGO_MANIFEST_DIR, binary-parent/source fallback, current_exe fallback, or omitted runtime exclusion is permitted.

Exact call chain: `bank_observation::observe:430` -> `RecoveryFrontier::observe_owned` -> frontier `observe` -> `state::paths` -> `state::_paths_for_bank:292`. That last function canonicalizes the actual runtime root and excludes recovery state beneath it even with an empty index/no hold. This trust check is not deferred. The remaining callee conflict was escalated with evidence: installation/plan.rs:1312-1335 records development source.path at the original source and active_binary=info.binary, while release uses locations.runtime/bin/am; installation.rs:410-420 source_binary consequently returns development target/release/am versus release bin/am. Owner must clarify receipt-authorized executing-binary selection or supply the real verified runtime-binary API; no nonexistent bin path is guessed.

Once the installation callee is authorized, append both actual registrations to the single `cli::register_providers` composition point, sharing one real `Arc<FrontierProvider>`:

- `bank_writes::register_recovery_frontier(Some(provider.clone()))`.
- `bank_contract::register_recovery_coverage(Some(provider))`.

The executing-binary callback must come from actual receipt ownership, never host current_exe fallback. Existing installation/providers.rs absent-receipt and binary-mismatch messages currently say pass --pack PATH rather than approved bootstrap guidance; the exact minimal owner-owned callee correction was requested. The active `no_receipt_runtime_location_refuses_with_bootstrap_guidance_and_preserves_sources` scenario records the approved contract without executing it. No new register_providers helper or startup domain operation is needed. The pending-file removals are already authored; composition is the remaining unapplied lane edit.

The three shared declarations are applied with approval. Only the installation owner's executing-binary/guidance decision and corresponding final composition remain intentionally unapplied. They are not disguised as missing production functions; no complete verification claim precedes them.

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

Public constants: HOLD, NESTED, TRANSACTION, SENTINEL, TERMINAL, UNIT_FIELDS. Public APIs are the table's functions, typed `VerifiedHold`, `SourceMarkerGuard`, `FrontierProvider::new(runtime_root_callback, executing_binary_callback)`, and `LayoutResolution` registration.

`FrontierProvider` implements **every method** of both assigned existing traits:

- `bank_writes::RecoveryFrontier`: capture_scope, observe, observe_owned, verified_hold, check, require_scope, observe_bank, ledger, evidence, validate_quiescence, verified_hold_for_runtime, verified_enrollment_hold_at_bank, observe_enrollment_bank, observe_source_markers.
- `bank_contract::RecoveryCoverage`: public_coverage.

New actual-callee interface in owned module: `LayoutResolution`, with required advance_ledger, restore_hold, retire, resolve methods and immediately following registry accessor. Future implementer is layout_resolution. `bank_writes::RecoveryTransactions::recover_nested` is a required append to the existing actual-callee interface, future implementer recovery_transactions. No implementation exists at this base, so no existing implementer/caller was left stale.

Reuse actual current `repository_config::SecurityScan` for pending/reservation safety and actual ordinary scope/identity APIs. The security lane is not present in this branch; no Security/CLASSES duplicate or temporary model_reference::CLASSES dependency was added. Existing bank_writes::Security audits remain owned by their actual writer callees, not replaced by frontier.

No baseline CLI parser belongs to recovery_frontier, and no artificial command/helper route is introduced.

## Tests, ignores and exact obligation accounting

Focused target: `t1_recovery_frontier`. Authored declarations: 16 active tests and one explicitly ignored real-security-dependent capacity test; these are declaration counts, not passing results. The active no-receipt locator scenario asserts the owner-approved bootstrap guidance and retained source bytes; the existing installation callee still needs its authorized guidance correction before that new assertion can pass.

Tests reuse merged `tests/support::World` for canonical private roots, isolated full HOME/config/state/Git environment and actual finite-deadline owned re-exec children. Filesystem fixtures are retained canonical baseline state snapshots, not providers or verified-hold capabilities. They exercise real marker files, immutable ledgers/complete predecessor chains, external bindings, source originals, private modes, hardlinks/symlinks, corrupt/deleted index, missing/foreign sentinels, missing holds, review/proposal/original changes, duplicate units/revision loss, partial terminal receipts, exact ranges/framing/references, proof digest behavior, capacity-only successors, empty lifecycle identity, extremal capacity and actual kernel writer-lock contention/nested/unwound source-marker scopes. No external network/credential is required. The only child launcher is World's bounded owned process API; no new child registry or isolation convention exists.

The ignored reservation test preserves its complete domain body and uses `cli::register_providers`, not a fake scanner. It is ignored precisely for the actual security scanner absent at681. Its full baseline public layout/admission contract remains partial even after the direct capacity body can run.

All frontier-related existing ignored test declarations were inspected, together with fixture/provider setup. **None waits solely for recovery_frontier** at681, so no existing body was removed/narrowed or ignore falsely lifted:

- Four `t1_bank` writer tests still lack real security and scope/provider fixture prerequisites; their existing setup registers only observation and constructs write proposals, not a bound principal/project. Frontier alone cannot turn them into authorized writers.
- `t1_bank_transactions::real_partial_rename_converges_finish_and_rollback_at_every_file_boundary` still needs the actual security scanner; full existing converge body is retained.
- Thirteen `t1_scope_binding` ignores retain genuine security absence; their real bind/preview/apply/enrollment fixtures reach the scanner before the tested operation. Scope/project/turn providers already exist in the CLI but are not substitutes for security. Two enrollment tests additionally exercise actual hold/source prerequisites.
- Two frontier-named `t1_private_state` ignores still require actual PrivateStateHooks (OMP/admission/installation composite) and security. Native absence must not be faked from ordinary source registries.
- Four `t1_turn_sources` ignored registration/ACK/wake/cap/publication bodies still require actual security for fixture principal/project binding and a valid executing runtime locator/installation. Installation's presence as a module does not manufacture a receipt owning the test executable.
- Raw nested identity has a separate real LayoutObservation ignore; this lane does not supply that callee.

The TSV enumerates every exact obligation from the three baseline frontier/frontier_closure/frontier_substitution suites (25 total), plus the exact large-owned-marker eligibility obligation. Six partial annotations reflect active local contracts or the one authored ignored capacity body. Twenty are explicitly unported public campaigns, not narrowed into local tests or promoted to implementation-only. Existing held-enrollment/identity/OMP obligations retain their exact pending statuses/reasons in the applicable reports; this lane claims none of those native/held campaigns qualified.

## Declared deltas and safety

- Runtime locations are explicit typed inputs/composition callbacks, replacing Python __file__ inference. Owner explicitly approved installed-v3-receipt-only root resolution and a bootstrap-directed no-receipt refusal; uninstalled source-root fallback is intentionally not supported.
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
4. Focused affected consumers: `t1_bank`, `t1_bank_observation`, `t1_bank_transactions`, `t1_scope_binding`, `t1_private_state`, `t1_turn_sources`, and `wiring`, with only genuinely satisfied ignores lifted by their owners.
5. Full `cargo test --offline --locked --no-fail-fast --features dev-tools` per parent integration policy, GNU and musl; independent Opus source/behavior acceptance.
6. Required source panic/unsafe scan over `src/recovery_frontier.rs`; no scan result is fabricated here.
7. Parent extracts only TSV partial rows' exact IDs and runs `am-dev obligations mark --status partial --ids-file ...`; twenty unported rows remain unported. Actual proof and any stronger status belong to parent.
