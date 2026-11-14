# Offline contract tests

From a clean checkout with a Rust toolchain and Git, run on each release triple:

```sh
cargo test --features dev-tools                                         # aarch64-apple-darwin
cargo test --features dev-tools --target x86_64-unknown-linux-musl      # Linux x86_64
cargo test --features dev-tools --target aarch64-unknown-linux-musl     # Linux arm64
cargo test --features dev-tools --test t2_cli                           # one test target
```

Add `--offline` when the dependencies in `Cargo.lock` are already fetched. The
`dev-tools` feature builds the developer-only `am-dev` binary (release building
and the obligations ledger); it is never shipped. Tests run the real `am` built
by Cargo. A passing run establishes the coverage of the tests that exist; it
does not by itself establish release readiness. Release acceptance also requires
the story-specific independent verdicts and delivery evidence in the
[execution contract](../design/v0.0.1-execution.md).

Every baseline obligation is accounted for in
`tests/obligations/python-baseline-v1.json`: 1,862 test methods, plus the
incidents, both matrix files and the evaluation entry points. Each test method
is `unported`, `partial`, `ported`, `implementation-only`, `blocker` or
`not-applicable`;
`tests/obligations_ledger.rs` checks the ledger's consistency. Report the
current state with:

```sh
cargo run --features dev-tools --bin am-dev -- obligations summary
```

An unported or partial obligation is not covered by a passing `cargo test`.

The [portable lifecycle matrix](../contracts/portable-lifecycle.md) binds every
approved lifecycle obligation to executable cases and adds installed
copy/all-client and link/custom-only journeys. Routine qualification runs locally
on the available macOS and Linux machines, retaining source-bound results. The
manually dispatched CI workflow (`.github/workflows/contracts.yml`) runs
`cargo fmt --check` and `cargo test --locked` on `aarch64-apple-darwin`,
`x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` runners and builds
the static musl release binaries; a final job requires every test and build job
to succeed. Pushes and releases do not automatically start it. A per-branch
concurrency group queues a later manual run rather than overlapping or
cancelling the running one.

### Baseline gate obligations

The baseline gate's contract remains a release obligation, recorded in the
ledger: a release gate MUST fail until every required contract is implemented,
nonempty, executed without skips or expected failures, and passing; each suite
keeps a 300-second deadline; a partial shard or batch is never release-complete;
completeness is claimed only from aggregated actual receipts bound to one
observed platform cell and source hash; batching MUST NOT change assertions,
omit subcases, introduce skips or raise a deadline; and expensive interruption
cases require every declared batch's actual fault proofs for the same runtime
artifact and write inventory, with no missing or duplicate
interruption/recovery/retry. Worker concurrency or registry validation alone
never establishes that a suite ran or that the matrix fits its budget.

The [shared-learning matrix](../contracts/multi-machine-learning.md) retains all
approved multi-machine obligations and adds installed-command journeys for
compatible edits, simultaneous publication with duplicate consolidation, and
cross-developer contradictions. It preserves feature state across independent
clones and a linked worktree, alongside the existing capture, initialization,
pruning, offline recovery and later-code-merge contracts.

Tests SHOULD invoke the shipped `am` binary and ordinary files; direct calls to
internal algorithms alone are not acceptance evidence. Optional stores are
explicitly outside v0.0.1; they are not a skipped required test.

The [first-release safety canaries](canaries.md) cover structural routes,
lifecycle selection, project isolation, shared developer attribution,
terminal deletion and poisoned imports through a copied portable skill.
Their guide records the current public surfaces and required expansion as
governed operations arrive.

The [baseline retrieval contract](../contracts/retrieval.md) adds public recall
cases for hard filters before ranking, exact/text explanations, nested conditions,
optional-index loss, safe source handling and independently bound local clones.

The [source-bound recall qualification controllers](recall-qualification.md)
freeze integrated A/B/C builds, run the four-condition refresh and exact-input
48-operation memo protocol, and retain the unchanged literal/semantic quality
cases with separate native/model timing. Their receipts are platform-specific;
controller availability is not an acceptance or release claim.

The [sharing boundary contract](../contracts/sharing-boundary.md) exercises a
configured bank through public identity, initialization, checkpoint, observed
writes and interrupted recovery, plus Git path eligibility without exchange.

The [independent local-bank contract](../contracts/independent-local-bank.md)
proves initialization, checkpoint/resume, orientation lookup, validation, Git
version/export and bounded local recovery without federation or optional stores.

The [learning comparison contract](../contracts/comparison.md) covers alternate
wording, distinct conditions, source-bound judgment, newly arrived duplicates,
unsafe/unavailable sources and terminal deletion before a durable learning change.

The [admission contract](../contracts/admission.md) covers source classification,
uncertainty and quarantine, qualified automatic admission, exact-delta/evidence
checks, duplicate/conflict routing, replay and interrupted rollback.

The [consolidation contract](../contracts/consolidation.md) covers stable-ID
deduplication, complete source provenance, convergence, distinct conditions,
conflict deferral, terminal invalidation, nested routing and crash recovery.

The [governance contract](../contracts/governance.md) exercises correction,
supersession, revocation, real expiry, retention, explicit erasure and stronger
reactivation, including retained older scope, consumed-capture invalidation,
source changes, complete history removal, and interrupted receipt recovery.

The [context assembly contract](../contracts/assembly.md) adds complete metadata,
type diversity, duplicate provenance, byte and externally measured token budgets,
and indivisible unresolved conflict groups through the copied portable CLI.

The [bounded resume contract](../contracts/bounded-resume.md) selects only current
task/component context through exact recall, with no personal layer or optional
store dependency and with explicit pending sharing freshness.

`tests/support/world.rs` provides private temporary home, XDG, client,
repository, bank, cache, credential and backup roots under a fresh 0700
directory. It constructs its own subprocess environment instead of inheriting
credentials or live config, and only allows local-file Git transport. Tests run
commands through `World::am`/`World::git` and resolve write targets with
`World::inside`, which rejects paths that escape the world; only explicit
`AGENT_MEMORY_FAULT_*` overrides are accepted, and shell execution is disabled.
The factory can create bare remotes, independent clones and separate worktrees
using real Git commands. No installed agent-memory or existing bank is needed to
run the tests.

This is an isolated test harness, not an operating-system sandbox for untrusted
test code. Test code has ordinary process permissions and MUST obey these
interfaces. No network, live credentials, client apps or live Memory Banks are
needed by the default suite. Product fault/recovery fixtures are enabled as
their write operations ship; the harness already propagates command failures
and enforces bounded subprocess timeouts.

Cargo reports each test target's pass/fail counts; exit 0 means every selected
test passed. The baseline gate's `agent-memory/test-gate-v1` JSON receipts,
registry layers, `pending` entries and `release_complete` flag are ledger
obligations without a Rust runner yet; a Cargo run is not such a receipt.
Generated state is temporary and is not committed to the bank.

The [incident and guide evidence workflow](incidents-and-guides.md) retains actual
public-interface reproductions with work/acceptance/integration/delivery links,
observes quoted CLI examples in isolated worlds, and verifies explicit normative
rule maps and route preservation against preserved old sources. Meaning changes
still require actual independent review; a mechanical checker does not prove
English equivalence or replace AWA's delivery evidence.

### Disposable public smoke recipe

From a trusted development checkout with a Rust toolchain and Git, the following
synthetic public-command cases use disposable homes/registries/runtime/repository
roots and no provider/network/real incident data:

```sh
cargo test --features dev-tools --test t2_layout_recovery -- \
  reported_sizes_fail_normal_gate_and_public_pages_reconstruct_every_original \
  all_preserve_apply_keeps_metadata_feature_head_and_exact_original_index
```

They construct exactly 79,054-byte contextual guidance and a 28,478-byte
eight-policy unadmitted handoff, demonstrate the normal prerequisite refusal,
then exercise the built `am bank recover-layout` inspection/pages/review/apply.
The baseline's third case (pending repair that retains exact private originals
and qualified recall) is unported in the ledger.
The all-preserve case separates eligibility/completion from the pending case;
the latter retains every original, real canonical mandatory witness records
and all eight private units with incomplete qualified recall. Controlled test
attestations test binding, not independent acceptance. Run/retain actual results;
authored cases alone do not prove repair of the unavailable original incident,
platform/legacy-reader crash behavior, native provider sessions or release
publication/fresh-second-clone acceptance.

The baseline `layout-recovery-legacy-consumers` contract used the complete
published v0.0.14 portable archive offline, checking its release-asset digest and
every archived file's bytes and mode before execution. It exercised the actual
old CLI against a pending layout hold and checked preservation of the bank,
private recovery operation, staged index, HEAD and feature bytes. This is a
cooperative mediated-reader check, not a fence against direct edits, already
running readers, other installations or copied checkouts. The fixture records
release provenance; it does not claim independent signature verification. It is
unported in the ledger; the legacy runtime archives leave the tree and are
replaced by captured state snapshots
([cutover design](../design/v0.0.15-rust-cutover.md#coverage-obligations-and-legacy-fixtures)).

An additional disposable public-CLI observation exercised a genuine normal
repository-evidenced admission whose one reviewed canonical record covered all
eight pending units: the terminal ledger linked eight distinct resolution
receipts to the same durable admission operation. Its synthetic source,
staged index, unstaged feature bytes and HEAD were unchanged; original bank
bytes remained in the private archive. The raw local transcript is retained at
`/tmp/am244-one-admission-smoke-raw.txt`. A separate sequential eight-admission
run is retained at `/tmp/am244-consumer-smoke-raw-2.txt`; it took 500.97 seconds
and did not satisfy the unchanged 300-second worker deadline. Only the separate
single-admission route above supplied the bounded eight-unit mechanics proof.
These controlled attestations are not independent semantic review.
An additional disposable comparison control
(`/tmp/am244-negative-smoke-raw-2.txt`) observed an unconfirmed capture remain
pending, found the pending unit despite two lexical misses, accepted the
unaltered source-bound proof, and refused both an omitted decision and a
substituted frontier. A first attempt used only one search and was rejected by
the existing two-search input contract; its raw transcript remains at
`/tmp/am244-negative-smoke-raw.txt`.

The corresponding registered regression has not yet been run as a suite.

The catalog frontier-only disposable observation used enabled and disabled
recall across a reviewed non-guidance terminal resolution with identical
canonical source hashes (`/tmp/am244-catalog-smoke-raw-2.txt`). This standalone
candidate did not produce an authenticated warm catalog hit. Its warm-hit
assertion remains required; no combined-candidate catalog qualification is
inferred from the presence of the separately integrated #206 changes.
Exact 16,384/16,385 source bytes are covered by the bounded bank-observation
tests (`tests/t1_bank_observation.rs`). The 64/65 mandatory-row and exact
65,536/65,537 disclosure/proof edges are not claimed by these observations:
mandatory worst-case proof reservation can refuse well before 64 pending rows,
and exact encoded sizes must be demonstrated with valid complete artifacts,
not patched limits or fabricated row counts.

The inventory-bound tests in `tests/t2_layout_recovery_observation.rs` exercise the actual raw
observer with 67,108,864 source bytes, 65,536 ordinary source files and 131,072
physical directory entries, then one over each limit. They check the complete
accepted inventory and that refusal preserves every source/directory. These
real-count API checks do not lower constants, substitute fake counts or prove
installed/native/hosted publication. The existing public file/depth tests
separately cover those dimensions.

The [2026-11-06 partial recovery record](../research/data/2026-11-06-layout-recovery/qualification.json)
retains the public recovery, frontier/comparison and combined
reconciliation/nested/publication/retention results. The combined run passed all
28 methods but used a longer group harness; it does **not** prove each registered
suite fits its unchanged 300-second release deadline. Full platform,
story-specific and signed-delivery evidence remains separate.
The separate [original-worker checks](../research/data/2026-11-06-layout-recovery/original-worker-budgets.json)
passed publication and retention within 300 seconds but timed out both
reconciliation modules. The expanded public recovery suite, including three
real-count boundary methods, separately passed all 12 methods in 216.71 seconds.
The [registered-batch receipts](../research/data/2026-11-06-layout-recovery/registered-worker-batches.json)
retain the subsequent complete-case rebatching: all five reconciliation batches
and all six nested-reconciliation batches passed their unchanged 300-second
worker deadline. The nested retirement extraction preserves the original six
fault subcases and assertions. Those results do not erase the original timeouts
or prove the remaining platform, hosted-authority or signed-delivery criteria.

