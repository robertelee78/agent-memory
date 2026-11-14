# Checkpoint and inactive lane report

## Source-ready boundary

Worktree `/opt/agent-memory-rust-checkpoint-inactive`, branch `lane/checkpoint-inactive`, approved frozen base `681c3b7f3e9cff5506709fef7a9a4fb8e4d5b31c`.

All 293 lines of baseline `checkpoint.py` and 131 lines of baseline `inactive.py` were read, with their complete direct contract tests, existing Rust consumers, provider interfaces, fixture conventions, cutover design, ADR-0029 and relevant prior lane reports. No Python, build, test, formatter, lint, smoke, Git mutation, tracker or Vox command was run. No passing-check or integration-proof claim is made.

Owned implementation, CLI, tests and approved minimal caller changes are settled at this boundary. **The real `Security::audit_records` adapter is not yet present at this base.** The integration owner approved rebasing onto the security integration after this boundary; only the parent performs that Git operation. This lane then must implement `audit_records` in the actual security provider by delegating to its real durable audit implementation, preserving explicit reasons/record IDs/action. It must not be replaced by a default body, fake provider, second registry or fabricated receipt. Final compilation/proof waits for this clean cutover. Security is unavailable at the frozen base, not represented as implemented here.

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
- `src/bank_transactions.rs`: exactly the five checkpoint preparation consumer calls use `validate_preparation`, not single-file `validate_write`.
- `src/handoff.rs`: `Checkpoint::list_text` returns `Result<String>`; its sole comparison caller propagates refusal with `?`.
- `tests/t1_a2.rs`: existing local Parser implementation follows the fallible signature, with `Ok(String::new())`; its unrelated test behavior is unchanged.
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

Future implementer: the **actual security provider from security integration**, not a new registry. Inactive passes category `retrieval`, phase `checked`, source class `external`, empty reasons, sorted unique record IDs and action `policy-check`, reproducing `security.audit` defaults at baseline `security.py:72`. Its return is the actual audit path/verified/phase receipt, not unit or fabricated success. This is the one remaining source integration step requiring the owner's upcoming security base.

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

Active tests cover every parser/render boundary with source-preserving output or refusal, exact UTF-8 rendered limit/idempotence, all ATX unknown levels/forms, fenced multiline lists, raw HTML/setext/control/nested-heading refusal, malformed input/wrapper rejection, real bounded input preservation, and actual native CLI five-field tombstone compaction/redaction. They reuse `tests/support::World` and existing record/identity fixture helpers; there is no second isolation convention.

Complete real-provider integration test bodies remain precisely ignored at this frozen base:

- `public_checkpoint_preserves_bank_code_index_and_provenance`
- `public_stale_proposals_preserve_manual_and_other_bank_edits`
- `interrupted_checkpoint_recovers_then_replays_without_later_overwrite`
- `inactive_compaction_removes_body_noise_not_required_conditions`
- `unsafe_required_metadata_refuses_without_echo_or_invention`
- `derived_rebuild_only_returns_canonical_active_version_references`
- `later_terminal_erasure_and_invented_ids_cannot_resurrect_cached_body`
- `scope_busy_bad_input_and_audit_symlink_preserve_canonical_bytes`
- `omitted_context_preparation_is_separate_then_checkpoint_changes_only_active`
- `copied_native_binary_preserves_untrusted_markdown_and_refuses_actor_switch`
- `git_carried_terminal_marker_survives_private_state_loss_and_rebinding`
- `missing_context_preparation_preserves_original_actor_during_resolution_switch`
- `public_markdown_matrix_and_utf8_boundary_preserve_every_original_source`

Dependencies are the real security scan/audit provider, recovery frontier and receipt-authenticated executing runtime for the existing canonical barrier/binding chain. Ignored tests do not install fake providers. The preparation worker re-executes under World and applies the actual typed transaction. The identity-resolution race delays the second **real Git attribution query** with a bounded shell rendezvous and then delegates all output to the real Git binary; no scope result or success proof is mocked. Public Markdown matrix retains the full CLI/write/readback/idempotency/refusal-preservation counterparts of direct parser tests. Copied-binary and clone/private-state-loss scenarios are native, not executable Python-artifact copies.

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

Observed commands: source/document/reference reads, edit/write operations, and one `jq` extraction of the required TSV from the existing obligation ledger. No verification was run by this worker.

After the parent rebases this frozen source onto the authorized security base and this lane adds the real audit adapter, the parent owns:

1. `cargo fmt --check` (or its single coordinated formatting pass).
2. `cargo check --offline --all-targets --features dev-tools`.
3. `cargo test --offline --features dev-tools --target x86_64-unknown-linux-gnu --test t1_checkpoint_inactive` three consecutive runs.
4. The equivalent three runs for `x86_64-unknown-linux-musl`.
5. `cargo test --offline --features dev-tools --test t1_a2 --test wiring` for the approved interface consumers/composition.
6. Full coordinated suites and independent acceptance, with actual runtime authority supplied before enabling the dependency-ignored journeys.
7. Parent-controlled obligation marking using the TSV; ignored/incomplete IDs remain partial.

No GNU/musl/macOS/aarch64, installed-release, crash-recovery or independent-acceptance proof is asserted by this source handoff.
