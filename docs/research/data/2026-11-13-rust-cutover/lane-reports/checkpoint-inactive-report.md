# Checkpoint and inactive lane report

## Source-ready boundary

Worktree `/opt/agent-memory-rust-checkpoint-inactive`, branch `lane/checkpoint-inactive`. Initial approved base was `681c3b7f3e9cff5506709fef7a9a4fb8e4d5b31c`; parent rebased the frozen source onto security integration `a725b97b34033221d8927cc7ecb95ddeaf694130` (parent-reported rebased checkpoint commit `f69fcbc`). This report describes the source after completing the real audit adapter on that new base.

All 293 lines of baseline `checkpoint.py` and 131 lines of baseline `inactive.py` were read, with their complete direct contract tests, existing Rust consumers, provider interfaces, fixture conventions, cutover design, ADR-0029 and relevant prior lane reports. No Python, build, test, formatter, lint, smoke, Git mutation, tracker or Vox command was run. No passing-check or integration-proof claim is made.

Owned implementation, CLI, tests and approved minimal caller changes are settled. The actual integrated `security::Security` now implements `bank_writes::Security::audit_records` by forwarding every argument to the real durable `security::audit`, with ordinary `bank_location=None`. No registry, default, substitute scanner, receipt or scope authority was added. Parent performed the rebase; this worker ran no Git command. Build/proof remains parent-owned.

## Exact authored files

New:

- `src/checkpoint.rs`
- `src/inactive.rs`
- `src/cli/helpers/checkpoint.rs`
- `src/cli/helpers/inactive.rs`
- `tests/t1_checkpoint_inactive.rs`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/checkpoint-inactive-report.md`

Modified:

- `src/lib.rs`: two module declarations.
- `src/bank_writes.rs`: required `Checkpoint::validate_preparation` and `Security::audit_records` methods appended to the existing callee traits.
- `src/security.rs`: implement the approved required `Security::audit_records` method on the real existing provider, forwarding reasons/record IDs/action without loss.
- `src/bank_transactions.rs`: exactly the five checkpoint preparation consumer calls use `validate_preparation`, not single-file `validate_write`.
- `src/handoff.rs`: `Checkpoint::list_text` returns `Result<String>`; its sole comparison caller propagates refusal with `?`.
- `tests/t1_a2.rs`: existing local Parser implementation follows the fallible signature, with `Ok(String::new())`; its unrelated test behavior is unchanged.
- `tests/support/mod.rs`: parent-approved `#[allow(unused_imports)]` on the intentional `pub use world` re-export only; no production or blanket warning suppression.
- `src/cli/helpers/mod.rs`: two helper declarations.
- `src/cli/table.rs`: actual `checkpoint` and `inactive-records` routes in baseline order.
- `src/cli/mod.rs`: one concrete `Arc<checkpoint::Checkpoint>`, cloned/unsized into the two actual registries at the existing single composition point.
- `tests/wiring-pending.txt`: remove the two now-implemented checkpoint registration entries only.

Outside the repository: `/tmp/checkpoint-inactive-obligation-ids.txt`, a TSV of intended status, exact baseline obligation ID and reason. The worker does not alter the obligation ledger or run `am-dev`.

## Public API and complete baseline implementation map

`checkpoint` exports `FIELDS`, `INPUT`, `EXTRA`, `LIMIT`, `MAX_ACTIVE`, `result`, `headings`, `text`, `list_text`, `input_value`, `render`, `inspect`, `preview`, `apply`, `current_router`, `validate_write`, `validate_preparation`, `Checkpoint`, and `command_result`.

- `headings`: all ATX heading levels delimit preserved sections; only exact H2 reserved names are managed. Indentation, tabs, closing hashes, fences, nested-list ambiguity, setext boundaries, raw HTML and control/Unicode separator refusals are retained. CR, LF and CRLF boundaries are recognized. Offsets are UTF-8 bytes, matching the existing handoff interface.
- `text`, `list_text`, `input_value`: original useful-Markdown bounds, normalized heading/fence validation, nested continuation indentation, sixteen-item/unique-evidence constraints, condition validation and SHA/null rules. The actual list renderer is fallible, including through handoff; invalid evidence cannot be normalized into apparent success.
- `render`: source-preserving replacement of managed sections only, unknown-section/fenced-example preservation, original ordering for appended managed fields, exact sorted/indented ASCII JSON for conditions/provenance/evidence observation, historical observation reuse, duplicates/ambiguous final boundaries and exact 16,384-byte UTF-8 refusal.
- `inspect`: real bank snapshot, existing frontier/coverage seams, bounded pinned active-file read, final complete hash observation, route/source-hash/preparation/untrusted-data fields.
- `preview`: principal pinned before discovery; real frontier scope; attribution/hash stability; real handoff evidence observations; existing security scan seam; actual common writer preview. Separate preparation builds the real full-bank transaction and updates only the original router and missing active context.
- `apply`: exact wrapper field/schema validation, source-bound proof consistency, only the observed active-context checkpoint target, reviewed rendering equality, real common writer/receipt/replay/currentness behavior and original meaningful output flags.
- `current_router`, `validate_write`, `validate_preparation`: original durable source hashes, original router bytes, exact rendered review proof, transaction file-set/preserved-file proof and optional fresh evidence pin checks. Transaction preparation never passes through a schema-dispatch fallback in the single-file validator.
- `command_result`: baseline checkpoint result/refusal envelope, capacity details and JSON styles.

`inactive` exports `INACTIVE`, `MAX_INPUT`, `INVALID_INPUT`, `bounded_file`, `compact`, `filter_projection`, and `command_result`.

- `bounded_file`: explicit 65,536-byte caller-input bound; canonicalized caller path then descriptor-pinned ordinary-file read, with original bytes retained on refusal.
- `compact`: existing records parser, exact terminal tombstone renderer, already-inactive statuses only, required conditional metadata/origin/state-reason/replacement preservation, optional admission/body removal, retained-metadata scan through the actual security seam, no canonical write or lifecycle assertion.
- `filter_projection`: exact request shape/project and 64-reference bound; unique stable identities and arbitrary-size positive integer versions (booleans/floats refused); real scope/snapshot/tombstone history; pinned re-read and hash verification of every canonical standalone record; only active canonical ID/version/path output; terminal/absent/inactive/wrong-version suppression; full scope/snapshot recheck; real required retrieval audit with sorted unique IDs and no cached prose/path/hash/metadata.
- `command_result`: content-free inactive refusal, no echoed removed bytes or input digest, capacity exception details and baseline write flags/JSON styles.

No baseline production function or meaningful branch in these two modules is intentionally omitted. Ordinary and held canonical write/transaction, receipt/replay, lock and turn-source barriers stay in the existing real writer/recovery seams; this lane does not duplicate their implementation or infer successful coverage from missing providers.

## Provider and authorized interface cutovers

One actual `checkpoint::Checkpoint` implements **both** existing checkpoint providers. The bank writer interface retains separate methods:

```rust
fn validate_write(&self, proposal: &Value, sources: bool) -> Result<()>;
fn validate_preparation(&self, proposal: &Value, sources: bool) -> Result<()>;
```

The five original Rust transaction call sites (base lines 399, 953, 1349, 1565, 1662) now invoke `validate_preparation` with their original arguments and source-check booleans. Baseline witnesses are `bank_transactions.py` lines 90, 362, 434, 474, 558 and 617; preparation and a single-file checkpoint have distinct authority/shape contracts. The production provider does not switch validators by input schema.

The existing handoff trait now requires:

```rust
fn list_text(&self, references: &[String]) -> Result<String>;
```

Complete Rust source/test reference inspection found the declaration at base `handoff.rs:24`, sole call at `handoff.rs:790`, the new provider and existing `tests/t1_a2.rs:2567-2569` Parser. All four were migrated, with no infallible coercion, empty fallback or panic.

The existing `bank_writes::Security` trait now additionally requires:

```rust
fn audit_records(
    &self,
    repo: &Path,
    operation_id: &str,
    category: &str,
    phase: &str,
    context: &Value,
    source_kinds: &[&str],
    reasons: &[String],
    record_ids: &[String],
    action: &str,
) -> Result<Value>;
```

The actual `security::Security` from the security integration implements this method. Inactive passes category `retrieval`, phase `checked`, source class `external`, empty reasons, sorted unique record IDs and action `policy-check`, reproducing `security.audit` defaults at baseline `security.py:72`. The adapter forwards all arguments and `bank_location=None` to the real `security::audit`; its return is the actual durable audit path/verified/phase receipt. There is no remaining security adapter implementation gap.

Existing real dependencies reused: bank writes/transactions/contract/observation, scope binding, repository-config SecurityScan, tombstones, records/frontmatter, handoff, local registry clock, recovery-frontier and public-coverage registries, turn-source canonical barriers and installation PackRoot. Missing providers retain refusal, never no-op success. No Security/CLASSES copy, guessed frontier API or sibling source was introduced.

## CLI declarations

- `am checkpoint inspect [--repo .]`
- `am checkpoint preview [--repo .] --input PATH`
- `am checkpoint prepare [--repo .] --input PATH`
- `am checkpoint apply --proposal PATH`
- `am inactive-records compact --file PATH`
- `am inactive-records filter --repo PATH --input PATH`

All reach actual typed domain functions and their original result/refusal envelopes. There is no Python wrapper, route-presence branching or unsupported success. Preparation returns the real typed transaction; the existing transaction subsystem owns its application. Runtime authority follows the owner's explicit decision: production canonical operations need the real installed-v3 receipt locator, not CARGO_MANIFEST_DIR, executable-parent guessing or an uninstalled callback fallback. This lane adds no runtime-location convention.

## Tests and obligations

Focused target: `t1_checkpoint_inactive`.

Active tests cover every parser/render boundary with source-preserving output or refusal, exact UTF-8 rendered limit/idempotence, all ATX unknown levels/forms, fenced multiline lists, raw HTML/setext/control/nested-heading refusal, malformed input/wrapper rejection, real bounded input preservation, and actual native CLI five-field tombstone compaction/redaction. Security-only ignores were removed from `inactive_compaction_removes_body_noise_not_required_conditions` and `unsafe_required_metadata_refuses_without_echo_or_invention`: `records::parse` standalone-record validation, inactive compaction, and the registered real SecurityScan do not call scope, canonical snapshots, frontier or runtime discovery. Their full existing bodies remain unchanged. A new active real-process audit-adapter contract verifies exact metadata bytes, all forwarded arguments, deduplication/order, 0600 mode, immutable retry and preserved collision evidence without invoking frontier. Tests reuse `tests/support::World` and existing record/identity fixture helpers; there is no second isolation convention.

Complete real-provider integration test bodies remain precisely ignored at this frozen base:

- `public_checkpoint_preserves_bank_code_index_and_provenance`
- `public_stale_proposals_preserve_manual_and_other_bank_edits`
- `interrupted_checkpoint_recovers_then_replays_without_later_overwrite`
- `derived_rebuild_only_returns_canonical_active_version_references`
- `later_terminal_erasure_and_invented_ids_cannot_resurrect_cached_body`
- `scope_busy_bad_input_and_audit_symlink_preserve_canonical_bytes`
- `omitted_context_preparation_is_separate_then_checkpoint_changes_only_active`
- `copied_native_binary_preserves_untrusted_markdown_and_refuses_actor_switch`
- `git_carried_terminal_marker_survives_private_state_loss_and_rebinding`
- `missing_context_preparation_preserves_original_actor_during_resolution_switch`
- `public_markdown_matrix_and_utf8_boundary_preserve_every_original_source`

Remaining dependencies are recovery frontier and receipt-authenticated executing runtime for the existing canonical observer/barrier/binding chain; security is implemented and registered, not a remaining dependency. Existing-bank fixture binding calls the canonical snapshot, and canonical observation unconditionally observes the real frontier; inactive filtering also performs full canonical snapshots/tombstone history. Checkpoint writes additionally use the real turn-source barrier. Thus those eleven remaining integration bodies stay precisely ignored, with no shortcut around source coverage or runtime-root exclusion. Their positive CLI journeys need a genuine installation fixture plus real frontier before enabling: same-process runtime callbacks cannot affect `World.am` children, and an absent installed-v3 receipt is not a missing registered module. No fake receipt, production environment fallback or replacement of public CLI assertions with library calls is supplied. The preparation worker re-executes under World and applies the actual typed transaction. The identity-resolution race delays the second **real Git attribution query** with a bounded shell rendezvous and then delegates all output to the real Git binary; no scope result or success proof is mocked. Public Markdown matrix retains full CLI/write/readback/idempotency/refusal-preservation counterparts of direct parser tests.

All **24 exact checkpoint/inactive baseline IDs are proposed partial**, including active narrower coverage and ignored integration bodies; none is claimed fully ported or proven merely because a test annotation exists. The TSV explains active coverage versus pending real dependency execution. There is no silently dropped baseline test. The parent should update these statuses only against actual executed proof and complete runtime prerequisites.

## Declared deltas and foundation

- No foundation changes, dependencies, unsafe code or production subprocess machinery were added.
- UTF-8-byte parser offsets follow the pre-existing handoff Rust contract, rather than Python character offsets; durable Markdown/JSON does not encode offsets.
- Existing cutover UTF-8/JSON surrogate/depth policies are reused. Accidental Python exception internals are replaced by typed/content-free refusals.
- Active context reads and caller input are explicitly bounded and pinned; namespace/source changes refuse rather than racing through stale paths. Canonical inactive source reads use the same explicit bounded reader. Unknown durable bytes remain evidence, never silently rebuilt.
- Caller-input symlinks are canonicalized before pinned reads, preserving baseline caller-selected target behavior without following unchecked canonical-state links.
- The handoff fallible-list signature and distinct preparation method repair port-interface mismatches; they are not relaxed baseline contracts.
- Installed-v3 receipt is the owner-approved sole production runtime locator. Uninstalled production canonical commands refuse; development installation is explicit. No claim is made that merely adding frontier/security modules satisfies that authority prerequisite in ignored CLI fixtures.

## Commands and parent verification plan

Observed commands: source/document/reference reads, edit/write operations, and the original `jq` extraction of the required TSV from the existing obligation ledger. After the parent rebase, source references were re-traced: the real Security provider is the sole writer-security implementer; the new checkpoint provider remains the sole writer-checkpoint implementer; all five transaction preparation calls and both fallible handoff implementations/caller retain the approved signatures. TSV dependency reasons were updated with `edit`. No verification was run by this worker.

The parent rebase and real audit-adapter implementation are complete. The parent now owns:

1. `cargo fmt --check` (or its single coordinated formatting pass).
2. `cargo check --offline --all-targets --features dev-tools`.
3. `cargo test --offline --features dev-tools --target x86_64-unknown-linux-gnu --test t1_checkpoint_inactive` three consecutive runs.
4. The equivalent three runs for `x86_64-unknown-linux-musl`.
5. `cargo test --offline --features dev-tools --test t1_a2 --test wiring` for the approved interface consumers/composition.
6. Full coordinated suites and independent acceptance, with actual runtime authority supplied before enabling the dependency-ignored journeys.
7. Parent-controlled obligation marking using the TSV; ignored/incomplete IDs remain partial.

No GNU/musl/macOS/aarch64, installed-release, crash-recovery or independent-acceptance proof is asserted by this source handoff.

## Independent source-review corrections

The parent relayed two P2 output-order findings and one P3 fixture finding from `ReviewCheckpointInactiveWave`. Both production result objects now preserve baseline insertion order: checkpoint apply emits `write` before `verified`, `writes_performed`, `changed_memory_paths` and `sharing`; inactive filter emits `remaining_references` before `discarded`. Derived receipt values are extracted before moving the receipt, without cloning the whole receipt; discarded count is calculated before moving the remaining-reference array.

The existing real apply and filter consumer journeys now assert the complete top-level key order from actual CLI output. They retain their genuine frontier/installed-runtime dependency ignores; no synthetic echo test or fake provider was added. The tombstone extra-field fixture now uses `replacen(..., 1)`, so only frontmatter receives the extra hash and the tombstone body remains empty. These corrections have not been built, formatted or executed by this worker. The audit adapter's bank-less real-process fixture also remains unexecuted pending parent proof.

## Parent-reported formatting and compile corrections

Parent reported `initial-fmt.log` formatting differences and `check.log` test-only E0603 failures for importing crate-private lifecycle helpers, with three accompanying E0282 inference errors and the intentional shared-support re-export warning. This worker did not rerun either check.

Applied the exact formatting replacements from the parent's diff, through hashline `edit`, to the five owned source/helper/test files named by that diff. No formatter was executed, upstream integration changes were preserved, and the accepted output-order/extra-field fixes were retained. The integration test now defines its own local `get`, `object` and `string` helpers following the existing test convention instead of widening production visibility. The shared support re-export receives only the parent-approved narrow `unused_imports` annotation. These corrections remain unexecuted pending the parent's next coordinated verification.

## Parent verification at a725b97

The source-only statements above describe the worker handoff, not the subsequent parent proof. Independent lead reviewer `ReviewCheckpointInactiveWave` (Opus 5.5 high) accepted the corrected source after the two output-order fixes and tombstone-fixture correction. The parent then applied the reported formatting and test-helper corrections without widening production visibility.

Observed on Linux x86_64 with rustc 1.94.1:

- `cargo fmt --check`: exit 0.
- `cargo check --offline --all-targets --features dev-tools`: exit 0, without warnings.
- GNU `t1_checkpoint_inactive`: **18 passed, 0 failed, 11 genuinely dependency-ignored**, three consecutive runs.
- GNU `t1_a2`: **80 passed, 0 failed**, three consecutive runs.
- GNU `wiring`: **2 passed, 0 failed, 1 ignored**, three consecutive runs.
- Musl `t1_checkpoint_inactive`: **18 passed, 0 failed, 11 genuinely dependency-ignored**, three consecutive runs.
- Musl `t1_a2`: **80 passed, 0 failed**, three consecutive runs.
- Musl `wiring`: **2 passed, 0 failed, 1 ignored**, three consecutive runs.
- Musl full `cargo test --offline --features dev-tools --target x86_64-unknown-linux-musl --no-fail-fast`: **899 passed, 0 failed, 60 ignored**, across 37 top-level harnesses. The 42 release-fixture tests that failed on GNU pass under the supported release target.
- GNU full `cargo test --offline --features dev-tools --target x86_64-unknown-linux-gnu --no-fail-fast`: **857 passed, 42 failed, 60 ignored**, across 37 top-level harnesses. Failures are confined to `t2_installation` (nine) and `t2_lifecycle` (33). Lifecycle failures explicitly report `Unsupported release target`; installation also includes assertion-only failures (false verification, wrong digest-refusal reason, or exit 0 instead of fault exit 86). All use signed-release fixtures, compile-time `install::TARGET` is GNU, and the owner-approved manifest contains only three macOS-arm64/Linux-musl targets. `[INFERENCE]` The assertion-only failures have the same early target-lookup cause. GNU remains a development proof target; Environment has no target-selection seam, so a test-only override is not available. This full run is not green. No baseline rerun was used to confirm it. Supported-target proof is recorded above; CI/proof-policy coordination remains necessary, without restoring GNU release support, faking binary identity or suppressing meaningful tests.
- Actual GNU `am checkpoint --help` and `am inactive-records --help`: exit 0; all four checkpoint subcommands and both inactive-records subcommands are present on the launched CLI.
- Production panic/unwrap/expect/placeholder scan of the two domain modules and two CLI helpers: no matches.

The parent marked the exact 24 obligation IDs partial using `am-dev`; none is promoted to fully ported by narrower passing tests. Proof commands, platform, status, independent source-review verdict and individual logs are retained in `/tmp/agent-memory-checkpoint-inactive-proof/`. Counts use only the final result per top-level Cargo harness, not nested fixture summaries. Initial formatter and compile failures preceded the successful checks and are disclosed above. No native macOS/aarch64, public release-signing chain or complete installed-runtime checkpoint/filter journey is claimed. Musl lifecycle fixtures execute actual compiled `am` bytes for their existing update/link smoke, but their signing and missing-callee services remain test fixtures, not a production-installation proof for the eleven ignored journeys.
