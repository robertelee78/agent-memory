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

`tests/contracts.json` routes the pack, lifecycle, bank, concurrency, client,
optional and evaluation layers. Each feature story MUST add public-interface
cases to its applicable suite and replace its pending entry with a test path.
Optional stores are explicitly outside v0.0.1; they are not a skipped required
test. An enabled but missing, empty, skipped, timed-out or failing suite MUST
fail both modes. Tests SHOULD invoke the shipped CLI, scripts and ordinary
files; direct calls to internal algorithms alone are not acceptance evidence.

The [first-release safety canaries](canaries.md) cover structural routes,
lifecycle selection, project isolation, shared developer attribution,
terminal deletion and poisoned imports through a copied portable skill.
Their guide records the current public surfaces and required expansion as
governed operations arrive.

The [baseline retrieval contract](../contracts/retrieval.md) adds public recall
cases for hard filters before ranking, exact/text explanations, nested conditions,
optional-index loss, safe source handling and independently bound local clones.

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
