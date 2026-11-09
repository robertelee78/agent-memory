# Portable lifecycle release matrix

The `portable-lifecycle` contract combines installed-command journeys with
the release gate's executed-case coverage for the complete approved
[ADR](../adr/0007-portable-pack-and-lifecycle.md) and
[blueprint matrix](../design/lifecycle-blueprint.md#verification-matrix).

The two new journeys exercise real local managed channels, temporary home/XDG
and client roots, actual installed commands and preserved source/index/history:

- Copy mode enables all discovery adapters plus a custom target, checks an exact
  repeat, recalls a qualified canonical record, performs a read-only update
  check and real update, verifies each client's artifact, deletes optional
  generated references, recalls again, uninstalls, resumes from plain Markdown,
  reinstalls the new revision, previews explicit purge without deletion and
  uninstalls again. Canonical memory and unfinished feature state remain intact.
- Link mode enables only two custom discovery targets, without optional client
  applications, a bank or developer setup. Actual status/doctor, read-only
  update check, update, unchanged repeat and uninstall retain neighboring skills
  and leave default client roots and identity/bank setup absent.

`tests/lifecycle-matrix.json` maps fifteen approved obligations to concrete
registered test methods. At complete-gate aggregation, each case must belong
to its declared required suite and current selector and appear among that
suite's actually successful executed identities. Missing, stale, misrouted or
unexecuted cases fail the receipt. Its references include no-client modes,
unrelated-target refusal, owned backups, sentinel edits, source/ref/downgrade
guards, all seventy-four real before/after fault workers, committed lost responses,
rollback/locks, modified-target preservation and confirmed purge. It also binds
package validation and the isolation/worker-collection controls. The collector
uses a supervisor that retains ownership of its process group through descendant
cleanup; it never signals a group after reaping its owner. Native ownership and
socket EOF regressions cover normal completion and timeout cleanup.
Failure to write the completion record also collects descendants and fails the
command rather than reporting an unverifiable success.
If that failure follows a worker timeout, the actual producer and disposable
gate regressions require both worker streams, the record exception and the
original timeout to survive collection.

The six expensive update/install phase-and-mode inventories run as eight
mandatory disjoint batches each; the four uninstall inventories use four each.
Every batch observes the complete clean write inventory, interrupts its selected
real writes, recovers, retries and retains the original preservation/ownership
checks. The gate compares their actual artifact identities and complete observed
inventories, then requires every point exactly once across its batches. A missing
batch, proof, interruption or retry fails the gate. The other ten fault workers
retain their complete inventories. No fault point or public 300-second worker
limit is removed.

Project identity discovery uses one coherent repository-settings observation
per discovery and reuses it when the saved authority already matches the
observed selection. Each final source/lock boundary still obtains a fresh
observation; there is no cache across operations or sessions.

Static package validation may reuse its single successful report for an exact
relative-path, byte and file-mode inventory at another absolute root. This
report conveys no source trust, installation ownership or runtime qualification.
Every inspection still reads the actual complete physical artifact on entry and
exit; declaration readbacks remain rooted in that candidate. Changed bytes,
modes, paths, unsafe topology or an in-bracket mutation cannot inherit old
validation. Returned reports are isolated, and failed brackets are not cached.

The three independent installed-bank cases have separate mandatory workers for
cold initialization/checkpoint, qualified recall and governance intent guards.
Their method bodies and assertions are preserved. The complete automatic
capture/publish/refresh journey remains one indivisible case; independent case
separation does not replace or split that workflow.

Six other expensive bank aggregates use exhaustive single-method registry
batches. Their original methods and scopes remain unchanged, and matrix
references name the batch that actually executes each required case. Complete
coverage and non-overlap are checked from real successful worker receipts,
not source-text wiring assertions.

Bindings are coverage obligations, not additional executions or proof that an
assertion is correct. The public gate executes each registered suite once;
independent acceptance checks the source, actual results and scope. The ordinary
300-second per-suite deadline, nonempty execution and rejection of skips and
expected failures remain unchanged.

## Run and retain evidence

From a checkout with Python 3.11+ and Git:

```sh
scripts/test.sh --release --json
```

The [CI workflow](../../.github/workflows/contracts.yml) runs this same complete
gate on Ubuntu and macOS with Python 3.11 and 3.15. It keeps JSON receipts,
unittest/fault stderr, the actual interpreter/platform and checked-out commit/tree
as artifacts even when a check fails. Each matrix cell must actually complete
successfully; creating a workflow, starting a run or passing a different Python
version is not a pass for that cell. Timeouts and failed runs remain evidence.
Gate failures identify the active suite and stage, retain exception chains and
include partial worker output when a timeout is followed by a collection error.
A disposable-copy negative test verifies that this combination remains a failure
with its original diagnostic evidence.

The workflow checks out code without persisting its repository credential and
pins the official [checkout](https://github.com/actions/checkout),
[Python setup](https://github.com/actions/setup-python) and
[artifact upload](https://github.com/actions/upload-artifact) actions to
release commits. CI orchestration obtains source/interpreters and uploads logs;
the actual product tests remain offline inside credential-free disposable
Worlds. No live client configuration, real developer bank or provider account is
used by those tests.

A complete contract receipt does not publish a release or replace milestone
acceptance. The multi-machine scenario has its own story, and the final release
gate still requires every other v0.0.1 story Done, an exact final candidate and
the actual non-draft/non-prerelease GitHub release.
