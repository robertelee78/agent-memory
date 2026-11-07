# Offline contract gate

From a clean checkout with Python 3.11+ and Git, run:

```sh
scripts/test.sh
scripts/test.sh --json
scripts/test.sh --release --json
```

The default mode runs every implemented suite and reports the remaining
contracts as `pending`. A default pass establishes incremental coverage only.
`--release` MUST fail until every v0.0.1 contract is implemented, nonempty,
executed without skips or expected failures, and passing. Release acceptance
also requires the story-specific independent verdicts and delivery evidence
in the [execution contract](../design/v0.0.1-execution.md); this test receipt
alone does not establish release readiness.

The [portable lifecycle matrix](../contracts/portable-lifecycle.md) binds every
approved lifecycle obligation to registered executable cases and adds installed
copy/all-client and link/custom-only journeys. Routine qualification runs locally
on the available macOS and Linux machines, retaining source-bound results.
The manually dispatched CI workflow can run the same complete gate on
Ubuntu/macOS and Python 3.11/3.15 when explicitly needed. Pushes and releases do
not automatically start that expensive matrix. Each run retains actual results,
failures and checked-out commit/tree evidence for each cell.

Each suite retains its 300-second worker deadline. The complete sequential
gate has a separate 180-minute CI job budget.
The registry explicitly recognizes the implemented `v0.0.15` feature suites;
their version scope does not make them optional or exempt them from that
deadline. Registry validation alone does not execute a suite.

For expensive independent interruption cases, the gate requires every declared
batch and aggregates actual fault proofs before reporting completeness. The
proofs must describe the same runtime artifact and observed write inventory,
with no missing or duplicate interruption/recovery/retry. Controlled gate
fixtures exercise rejection of absent/incomplete proofs; their simulated
metadata is not production interruption evidence.

The [shared-learning matrix](../contracts/multi-machine-learning.md) retains all
approved multi-machine obligations and adds installed-command journeys for
compatible edits, simultaneous publication with duplicate consolidation, and
cross-developer contradictions. It preserves feature state across independent
clones and a linked worktree, alongside the existing capture, initialization,
pruning, offline recovery and later-code-merge contracts.

`tests/contracts.json` routes the pack, lifecycle, bank, concurrency, client,
optional and evaluation layers. Each feature story MUST add public-interface
cases to its applicable suite and replace its pending entry with a test path.
Optional stores are explicitly outside v0.0.1; they are not a skipped required
test. An enabled but missing, empty, skipped, timed-out or failing suite MUST
fail both modes. Tests SHOULD invoke the shipped CLI, scripts and ordinary
files; direct calls to internal algorithms alone are not acceptance evidence.

An expensive contract file MAY be registered as several named batches using
`tests`, a nonempty array of TestCase class or `Class.test_method` selectors.
The worker's `--suite NAME` selects the registered batch. Before any test runs,
the worker MUST verify that all batches for that file are disjoint and cover
every discovered case; omissions, overlaps, empty or unknown selectors refuse.
The gate independently aggregates the actual per-batch case inventories and
execution counts before reporting completeness. Existing lifecycle write-fault
proofs remain separately required. Batching MUST NOT change assertions, omit
subcases, introduce skips or increase the 300-second worker deadline.

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

`tests/world.py` provides private temporary home, XDG, client, repository, bank,
runtime, cache, credential and backup roots. It constructs an allowlisted
subprocess environment instead of inheriting credentials or live config, and
only allows local-file Git transport. Tests MUST use `World.run`/`World.git`
for commands and `World.inside` for write targets. The factory can create bare
remotes, independent clones and separate worktrees using real Git commands.
It rejects escaping paths, symlink escapes and root/auth environment overrides.
Workers validate their world before loading suites. Temporary cleanup does not
follow symlinks out of the world. No installed agent-memory or existing bank is
needed to run the gate.

This is an isolated test harness, not an operating-system sandbox for untrusted
test code. Test code has ordinary process permissions and MUST obey these
interfaces. No network, live credentials, client apps or live Memory Banks are
needed by the default suite. Product fault/recovery fixtures are enabled as
their write operations ship; the harness already propagates command failures
and enforces bounded subprocess timeouts.

Receipts use `agent-memory/test-gate-v1`: each suite reports its layer, status
and test counts; `release_complete` remains false while a required contract is
pending or failed. `passed` applies to the selected mode. Exit 0 means the mode
passed, exit 1 means a failing/incomplete gate, and exit 2 means invalid command
arguments. Diagnostics and unittest output go to stderr; `--json` stdout is one
JSON document. Generated state is temporary and is not committed to the bank.

The [incident and guide evidence workflow](incidents-and-guides.md) retains actual
public-interface reproductions with work/acceptance/integration/delivery links,
observes quoted CLI examples in isolated worlds, and verifies explicit normative
rule maps and route preservation against preserved old sources. Meaning changes
still require actual independent review; a mechanical checker does not prove
English equivalence or replace AWA's delivery evidence.

### Disposable public smoke recipe

From a trusted development checkout with Python 3.11+ and Git, the following
synthetic public-command cases use disposable homes/registries/runtime/repository
roots and no provider/network/real incident data:

```sh
PYTHONPATH="$PWD/tests" python3 -B -m unittest \
  contracts.test_layout_recovery.LayoutRecoveryContract.test_reported_sizes_fail_normal_gate_and_public_pages_reconstruct_every_original \
  contracts.test_layout_recovery.LayoutRecoveryContract.test_all_preserve_apply_keeps_metadata_feature_head_and_exact_original_index \
  contracts.test_layout_recovery_frontier.RecoveryFrontierContract.test_reported_size_pending_repair_retains_exact_private_originals_and_qualified_recall
```

They construct exactly 79,054-byte contextual guidance and a 28,478-byte
eight-policy unadmitted handoff, demonstrate the normal prerequisite refusal,
then exercise installed `am bank recover-layout` inspection/pages/review/apply.
The all-preserve case separates eligibility/completion from the pending case;
the latter retains every original, real canonical mandatory witness records
and all eight private units with incomplete qualified recall. Controlled test
attestations test binding, not independent acceptance. Run/retain actual results;
authored cases alone do not prove repair of the unavailable original incident,
platform/legacy-reader crash behavior, native provider sessions or release
publication/fresh-second-clone acceptance.

The registered `layout-recovery-legacy-consumers` contract uses the complete
published v0.0.14 portable archive offline, checking its release-asset digest and
every archived file's bytes and mode before execution. It exercises the actual
old CLI against a pending layout hold and checks preservation of the bank,
private recovery operation, staged index, HEAD and feature bytes. This is a
cooperative mediated-reader check, not a fence against direct edits, already
running readers, other installations or copied checkouts. The fixture records
release provenance; it does not claim independent signature verification.
An additional disposable public-CLI observation exercised a genuine normal
repository-evidenced admission whose one reviewed canonical record covered all
eight pending units: the terminal ledger linked eight distinct resolution
receipts to the same durable admission operation. Its synthetic source,
staged index, unstaged feature bytes and HEAD were unchanged; original bank
bytes remained in the private archive. The raw local transcript is retained at
`/tmp/am244-one-admission-smoke-raw.txt`. A separate sequential eight-admission
run is retained at `/tmp/am244-consumer-smoke-raw-2.txt`. These controlled
attestations demonstrate consumer mechanics, not independent semantic review.
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
candidate did not produce an authenticated warm catalog hit; the registered
warm-hit assertion requires the separately integrated #206 catalog repair.
Exact 16,384/16,385 source bytes are already covered by
`test_bounded_bank_observation.py`. The 64/65 mandatory-row and exact
65,536/65,537 disclosure/proof edges are not claimed by these observations:
mandatory worst-case proof reservation can refuse well before 64 pending rows,
and exact encoded sizes must be demonstrated with valid complete artifacts,
not patched limits or fabricated row counts.

`LayoutRecoveryInventoryBounds` in the same module exercises the actual raw
observer with 67,108,864 source bytes, 65,536 ordinary source files and 131,072
physical directory entries, then one over each limit. It checks the complete
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

