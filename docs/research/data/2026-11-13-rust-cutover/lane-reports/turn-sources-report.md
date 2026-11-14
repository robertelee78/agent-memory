# Turn sources lane report

## Delivery boundary

Worktree `/opt/agent-memory-rust-turn-sources`, branch `lane/turn-sources`, assigned base `f7391170a0774f1589218025450346ff7e356f23`.

Implementation and parent GNU/musl verification are complete at this frozen base; native-harness qualification is not claimed. The implementation worker ran no build, test, lint, formatter, smoke, Python, commit/push, tracker or integration-branch command. Parent proof and delivery are distinguished below. Rust LSP is unavailable; the expressly authorized textual-reference fallback was used.

Exact authored files:

- `src/turn_sources.rs` (new)
- `tests/t1_turn_sources.rs` (new)
- `src/lib.rs` (one module declaration)
- `src/error.rs` (append-only publication refusal kind and constructors)
- `src/cli/mod.rs` (two production-provider registrations)
- `tests/wiring-pending.txt` (remove the two implemented source registrations; add the T2c PublicationGate dependency)
- `tests/obligations/python-baseline-v1.json` (parent-applied `am-dev` marking: 12 partial IDs; 12 deferred IDs unchanged)
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/turn-sources-report.md` (this requested report)

Outside the repository: `/tmp/turn-sources-obligation-ids.txt`, tab-separated intended status, exact baseline ID, and dependency reason where deferred. Parent confirmed successful `am-dev` marking from `/tmp/turn-sources-partial-ids.txt`: 12 IDs marked partial, with the 12 deferred IDs untouched. The worker did not mutate the ledger.

## Full baseline implementation map

The complete 985-line `turn_sources.py` and the baseline source/held-source contracts were read. All production functions/branches are represented; the dependency refusals below are not substitute implementations.

- `_HeldPrivateLock`, `private_lock`: reuse foundation `fs::PrivateLock`, descriptor/parent identity rechecks, private ownership/mode/link/type policy, nonblocking flock, and explicit successful exit recheck. Actual contention maps to `Kind::SourceTransportBusy`, preserving the distinct private/completion public messages. No guessed owner, lockfile deletion, permission repair, detached process or process group is introduced.
- `read_only_locks`: synchronous thread-local scope with unwind-safe restoration. Existing-only locks never create synchronization state. Read-only completion without a lock is permitted only under the live exclusive source barrier; directory binding and absence are rechecked afterward.
- `check_source_compatibility`, `source_barrier`, `_borrowed_source_barrier`, `canonical_writer_barrier`, `source_operation`: exact two-capability shared marker allowlist; checks before and after acquisition/body; reentrant reads share the actual locked descriptor; shared-to-exclusive upgrades refuse. Borrowing requires the source-specific unforgeable guard, the current thread and the registered descriptor identity. Changed inode/parent/marker refuses. Canonical acquisition alone has the baseline 30-second/25-ms bounded busy retry; the writer body is never replayed. `operation` is the Rust closure equivalent of the decorator.
- `_namespace`, `namespace`, `private_namespace`, `_enrollment_namespace`: actual scope-binding APIs and sealed `PrivateEnrollmentContext`, exact root/harness/session/lifecycle sorted-compact hash, existing state-path calculation, supported harnesses and UTF-8 byte/control bounds. Reused enrollment context needs its live read scope and already-held barrier. Ordinary completion never accepts private enrollment authority.
- `encoded`, `bounded`, `private_mkdir`, `read_bytes`, `read`, `sync`, `exclusive`: sorted compact UTF-8 JSON without newline; bounded pinned no-follow reads; ordinary single-link current-user files with no group/other access; file/parent binding rechecks; O_EXCL 0600 publication, file fsync, directory fsync and decoded readback. Occupied names remain reservations, not overwrite permission. Reads accept the baseline reader's whitespace/key ordering instead of demanding a re-encoding match.
- `ref`, `validate_reference`, `index`, `fault`: exact v2 reference bytes/schema/namespace hash, fixed 4096 slots, complete bounded name inventory, every occupied reference validated even after holes or a matching row, two attempts on changing inventory/O_EXCL collision, independent registration from completion lock, durable fault retention.
- `validate_source`, `enrollment_value`, `enrollment_state`, `held_register`, `register`: exact source/intent fields and schemas, independently minted observation UUID per ordinary callback, immutable digest/reference, wake hints only (never completion authority), hold-only real native event requirement, intent-before-index/source publication, all seven baseline hold-source fault boundaries, exact retry of original observation and hold pins, duplicate/conflicting native evidence refusal, ordered source-written/registered journaling, conservative unverified bookkeeping after failed held enrollment, complete source/index/readback/authority rechecks. Reusing a lifecycle or identical text never deduplicates ordinary callbacks; only the explicitly held native-event retry path does so.
- `validate_receipt`, `VerifiedReceipt`, `verify_assessment`, `complete`: exact no-learning assessment validator and published-assessment bounds; source/digest-bound private capability, immutable verification request, re-verification under completion lock, original-source uniqueness, exact completion/checksum bytes and completion bound, idempotent acknowledgment, final ordinary scope recheck. Incoming flags cannot construct `VerifiedReceipt`; publication linkage remains explicitly unverified. `PublicationGate` is needed only on the published branch, not for no-learning.
- `unidentified_inventory`, `retain_unidentified_input`, `enrollment_inventory`, `inventory`, `private_inventory`, `pending_rows`: complete bounded raw-input retention with base64/digest and original context pins, no invented event/observation, exact intent event namespace, completion checksum/receipt validation, retained malformed-slot/fault bookkeeping, exact fingerprint pin arrays, original intent crash-window pending rows, held contexts always incomplete/recovery-pending, complete retained-input IDs. The in-module base64 codec is pure data processing; no interpreter or extra dependency is used.
- `_inspect_enrollment`, `inspect`, `get`: real sealed enrollment read reuse, private lock, two complete locked inventory observations, entry/exit enrollment rechecks; `get_source` returns only the exact original source after removing completion metadata.
- `cleanup`, `reserve_wake`: remove only a complete matching reference, fsync and reobserve, restore index after a racing append; exact two-slot wake cap and per-fingerprint deduplication with fresh private authority checks.
- `backlog`: offset bounds, full index validation, project/harness/namespace checks, exact pending-row shapes and fault codes, 60,000-byte pagination, bounded/sorted intent and unidentified-input orphan discovery, separate enrollment offset, legacy codex/claude inventory and 4096 pending limit, exact legacy projection and final enrollment recheck. No scan infers source completeness.
- `pending_notice`: earlier-lifecycle filtering, exact summary/fingerprint, held-aware private notice namespace, completion lock, durable notice deduplication and fresh authority checks before temporary creation, before replacement and after directory synchronization.

Rust signatures use explicit `runtime_root: &Path` for domain operations that formerly located the executing pack through Python. `reference` and `get_source` are the Rust names of baseline `ref` and `get`; no existing Rust caller needs migration for those newly introduced functions. Typed APIs make malformed Python argument types and arbitrary receipt subclasses unrepresentable rather than granting authority.

## Public API and adapter wiring

Public constants: `MAX_SOURCES`, `MAX_TURNS`, `MAX_SOURCE`, `MAX_COMPLETION`, `MAX_INTENT`, `MAX_RETAINED_INPUT`, `HARNESSES`, `INTENT_SCHEMA`, `UNIDENTIFIED_SCHEMA`.

The module exports the baseline-named source, namespace, inventory, enrollment, backlog, wake, notice, assessment and completion operations described above; the Rust-specific descriptor-level entry is `source_barrier_at(base, exclusive, existing_only)`, which mints synchronization only, never scope/assessment authority. `SourceBarrier::recheck` and `SourceBarrier::borrowed` provide explicit observed exit checks. `VerifiedReceipt::value()` exposes advisory receipt data, not its private verification capability fields. `Namespace` contains root/bank/base/directory/identity data, never a scope capability.

One real unit provider, `turn_sources::TurnSources`, implements every existing method of both callee traits:

- `bank_writes::TurnSources`, including `CanonicalWriterBarrier`, transaction ordinary-bank fencing, existing-only source barrier, shared-marker compatibility and live-barrier observation.
- `turn_check::TurnSources`, creating the real refresh-notice private namespace under source barrier then completion lock; its `NoticeStore` implementation performs bounded reads, pinned durable replacement and observed removal with fresh private enrollment checks.

`cli::register_providers` directly installs both implementations. Their two old pending lines are removed. No parallel source/scope/private-state provider convention or redundant trait was added. Existing callers in bank writes, bank transactions, scope binding and private state therefore use this provider without consumer changes. The canonical writer adapter obtains its executing runtime from the existing `cli::PackRoot` accessor; until installation registers that real provider it refuses rather than guessing a source directory.

New T1-owned upward interface (only one registry):

```rust
pub trait PublicationGate: Send + Sync {
    fn request_value(&self, repo: &Path, request: &Value) -> Result<()>;
    fn replay(&self, repo: &Path, request: &Value, quarantine_records: bool) -> Result<Value>;
}
pub fn register_publication_gate(provider: Option<Arc<dyn PublicationGate>>);
```

The immediately following crate-visible `publication_gate_provider()` returns the registered implementation after releasing the registry lock. T2c's real `memory_publication`/`sharing_recovery` implementation must validate the entire request and replay the requested base/candidate on authority. It is not present at this base. Its pending line names both modules. Absence is `Kind::SourcePublicationUnavailable` with `PublicationGate provider is not registered; publication remains pending`, never a fabricated proof, no-op provider or default success.

No new CLI command is invented: this module is the T1 backend consumed by the existing writer/check entry points and future T2f harness adapters.

## Tests and obligation statuses

Target: `t1_turn_sources`. It contains active real-private-filesystem contracts for exact bytes/modes/O_EXCL bounds, append/completion lock independence, malformed reservations, source/receipt identity and completion checksum, exact native metadata, full pending-index validation past holes, bounded symlink/nonregular/FIFO refusal, kernel flock contention and release, reentrant/no-upgrade/borrowed guards, real inode replacement and foreign-thread refusal, compatibility entry/exit refusal, noncreating read-only locks, original enrollment intent/journal preservation, raw unidentified input bytes, cleanup/reindex and persistent capacity fault behavior.

Four explicitly ignored isolated-real-process tests contain actual repository/developer/project binding and production registration, ordinary callback registration, distinct/identical observations, idempotent completion, late backlog, two-wake cap, overbound assessment refusal, all 128 source slots/capacity fault/new-turn backlog, and missing publication-gate refusal. They need real security, recovery-frontier, installation and their safety dependencies; no fake provider or synthetic successful hold is registered. The re-exec helper uses a canonical 0700 root and isolated HOME/config/state/Git environment with finite child deadlines, and includes ignored tests explicitly in its child dispatch. Missing prerequisites cannot masquerade as an empty successful fixture.

Recommended intended ledger statuses are **partial**, not ported, for the 12 exact IDs marked partial in `/tmp/turn-sources-obligation-ids.txt`:

- Nine `test_turn_sources.py::Sources` IDs except the prompt-hook integration ID. Full ordinary registration/ACK/wake/cap scenarios are authored but await real providers. Active source/receipt/lock/index coverage does not claim the pending native callback/cleanup-race campaigns are qualified.
- `test_held_source_enrollment.py::HeldSourceEnrollment.test_shared_source_barrier_reentrancy_and_lifecycle_contention_preserve_sources`: real descriptor/reentrant/contention behavior active; full held-bank/process/source-preservation fixture awaits recovery.
- `test_held_source_enrollment.py::HeldSourceEnrollment.test_all_shared_readers_refuse_unknown_compatibility_before_source_index_access`: real marker/entry/exit preservation active; complete all-reader held integration awaits dependencies.
- `test_omp_stored_source_lifecycle.py::StoredSourceLifecycle.test_both_contended_lock_families_raise_typed_busy_before_body`: both actual flock families and exclusion/release are represented with independent descriptors. This lane does not claim native stored-source activation qualification.

The 12 **unported/deferred test obligations** in that file are explicit, not hidden partials: the source prompt-hook integration obligation and the remaining eleven held-enrollment obligations. Their exact dependency reasons accompany the IDs. Full held tests need real recovery observation/frontier/quiescence, retained manifest/proposal/ledger/runtime proofs, checkpoint (for its late writer race), and actual Codex/Claude hook/boundary implementations. The corresponding production branches and fault stages are implemented here; no fake verified-hold constructor, wiring-only test or mocked publication proof is substituted for those missing full dependencies. Existing OMP source-phase/stream/segment suites remain owned by T2f; the borrowed barrier required by them is implemented, but this report does not mark those campaigns ported.

No native harness, installed-release, GNU/musl, macOS or aarch64 qualification is established by these fixtures or this unexecuted implementation handoff.

## Deltas and foundation

- No foundation source changes, new crates, unsafe blocks, unbounded reads or production child processes.
- Runtime root is explicit; existing CLI PackRoot provides it to legacy writer/check trait adapters. Missing installation is a named existing provider refusal, not a guessed runtime fallback.
- Existing strict UTF-8/JSON depth/surrogate cutover policy is reused. Dynamic malformed argument types are represented by typed Rust signatures or refusals rather than Python exception internals.
- Durable source/reference/intent/completion/notice field sets and sorted-compact UTF-8 bytes remain unchanged. Descriptor-pinned observations and guarded notice/cleanup writes strengthen binding checks rather than changing authoritative content.
- Failure to prove namespace inventory stability refuses; the pending-slot allocator retains the baseline two attempts. File occupants, unknown private metadata and interrupted source/intent evidence are never silently erased or rebuilt.
- A failed private temporary publication may retain its incomplete diagnostic temporary rather than deleting unverifiable bytes. A successfully created/readable notice temporary is removed on a subsequent failure only through an observed pinned removal; a pre-existing collision is never deleted. No durable source/notice success is returned on that failure.
- Explicit stricter retained-input decoding delta: surplus base64 `=` padding after a complete four-character group is refused, including forms Python may accept with `validate=True`. Canonical baseline-written base64 remains readable; tampered bytes are preserved and surface `unidentified-input-unverifiable`, never an observation or completion.
- The source JSON reader intentionally retains baseline `developer_identity.load` non-finite behavior: NaN, Infinity, -Infinity and overflowing float lexemes remain accepted; sorted-compact encoding emits the corresponding baseline tokens. Duplicate keys still refuse. A dedicated filesystem regression records this policy; it is not silently replaced with finite-only decoding.

## Observed parent verification and independent acceptance

Host: Linux x86_64 kernel `7.0.11-76070011-generic`, `/tmp` tmpfs; rustc `1.94.1 (e408947bf 2026-03-25)`, cargo `1.94.1`.

- `cargo fmt --check`: pass.
- `cargo check --offline --all-targets --features dev-tools`: pass, no warnings.
- `cargo test --offline --features dev-tools --target <triple> --test t1_turn_sources`: **18 passed, 0 failed, 4 real-dependency ignored**, three consecutive runs for each of `x86_64-unknown-linux-gnu` and `x86_64-unknown-linux-musl`.
- `cargo test --offline --features dev-tools --target <triple> --test wiring`: **2 passed, 0 failed, 1 cutover-gate ignored**, three consecutive runs on each target.
- `cargo test --offline --features dev-tools --target <triple> --no-fail-fast`: **556 passed, 0 failed, 28 ignored** on each target, counting only the final result of each of 27 top-level harnesses. Nested re-executed fixture summaries are not additional tests or failures.
- Required production panic scan: no `unwrap()`, `expect(` or slice-range matches in `src/turn_sources.rs`.
- Parent `am-dev obligations mark --status partial --ids-file /tmp/turn-sources-partial-ids.txt`: pass. All 12 authored obligations remain partial; all 12 deferred rows are untouched.

Independent `anthropic/claude-opus-5-5:high` reviewer `ReviewTurnSourcesWave` initially required two P2 filesystem parity corrections: vanished pinned reads must remain absence, and completion locking must not create a missing namespace. Both were corrected with deterministic filesystem regressions. Targeted re-review: **ACCEPT**, source-only; no P2 remaining. Source review also records the explicit stricter surplus-base64-padding refusal and baseline non-finite JSON policy.

Initial compilation failed on an unconstrained closure error type; an explicit `Result<Option<Value>>` fixed inference. An intermediate wiring check correctly detected a private domain helper's `register_at` prefix; it was renamed to `append_source_at`, with both internal callers migrated, no startup operation, fake pending entry or wiring-test change. Initial formatting differences were corrected through edits.

Parent cancelled an earlier proof job when review findings arrived. It is not counted as a successful finish line. Initial compiler/wiring excerpts and the cancellation boundary are retained; individual named logs were subsequently reused by corrected final runs rather than archived in full. Final proof directory `/tmp/agent-memory-turn-sources-proof/` contains exact commands, platform, six focused source runs, six wiring runs, both full suites, final check/format and panic scan.

The four ignored real integration scenarios and the publication/installation/recovery dependencies remain explicit, not accepted behavior proof. No native harness, installed release, macOS or aarch64 qualification is claimed. The integration owner supplies additional targets and records the immutable candidate and independent verdict on #249 through awa; this lane does not advance whole-story acceptance.

### Parent initial verification correction

Parent reported initial formatting differences in `/tmp/agent-memory-turn-sources-initial-fmt.log` and compiler E0282/E0283 diagnostics in `/tmp/agent-memory-turn-sources-proof/check.log`. All primary compiler diagnostics in that log identify the same orphan-backlog closure's unconstrained error type; the referenced conversion implementations are diagnostic notes, not independent failures.

The supplied formatting changes were applied through anchored edits only to `src/turn_sources.rs`, `tests/t1_turn_sources.rs`, and the owned additions in `src/cli/mod.rs` and `src/error.rs`. The orphan-row result now explicitly declares `Result<Option<Value>>`; this changes inference only. No formatter, build or test was rerun by the worker. Parent subsequently completed the ledger marking and final verification reported above.

### Independent source-review corrections

Parent stopped its in-flight proof before these edits; interrupted evidence logs were not changed by the worker. The review identified two P2 parity defects, now addressed:

- `read_bytes` maps NotFound during pinned parent opening, file opening and final named-file observation to absence, as baseline FileNotFoundError handlers do. Final named absence is resolved before propagating a descriptor-read change refusal, preventing an unlinked source reservation from becoming a permanent registration fault.
- `locked` pins and validates an already-existing private namespace, creates/opens only `completion.lock` relative to that pinned descriptor, and rechecks the directory and locked inode before/after its body. It never calls a directory-creating lock helper for completion. Missing namespaces refuse without parent creation, body execution, or replacement of the source-missing fingerprint.

Added deterministic real-filesystem contracts for missing noncreating completion locks, symlink refusal and ordinary existing-directory locking; stable file/parent disappearance without fault artifacts; preserved refusal of surplus retained-input base64 padding; and baseline non-finite JSON tokens. Stable absence is exercised without timing sleeps or test-only production hooks; no deterministic mid-read syscall interception is claimed. No checks or formatters were run by the worker. Parent owns the next affected proof and targeted review verdict.

### Private helper naming correction

Parent reported formatting/check success and `t1_turn_sources` at 18 passed / 4 ignored before the wiring target failed on the private domain helper name `register_at` (`/tmp/agent-memory-turn-sources-proof/x86_64-unknown-linux-gnu-wiring-1.log`). A repository-wide reference scan found exactly its private definition and two internal callers. They now use `append_source_at`, with no compatibility alias, visibility change, startup operation or fake pending registration. No test, wiring policy or public API changed; the worker ran no checks or formatter. Parent stopped the proof before the full suite and owns the next verification result.
