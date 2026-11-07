# #209 checked-page export — development evidence

Runtime candidate `8956aa40c8ad70f33b7fb2df14d2f64a6d43a278`, integrated base
`27aa667d4d9e225974f03f90e47bb878439b223b`. Work key remains
`robertelee78/agent-memory:ADR-0009:bounded-24-session-stress`.
The operator explicitly authorized this bounded engineering fix after Fable
review; no new numeric latency promise or broader cache scope is inferred.

## Observed checks

| Contract suite | Passing methods | Failures/errors/skips |
| --- | ---: | ---: |
| Checked-page export | 5 | 0 |
| Existing semantic sharing | 9 | 0 |
| Existing recall catalog | 17 | 0 |
| Existing refresh-v2 | 2 | 0 |

A twelve-command actual public-CLI smoke passed exact export/independent-page
byte agreement, reviewed publication, useful peer refresh and publisher bank,
feature/index/HEAD preservation. cProfile observed one frontier build per export
and one per independent page-v2 request. Reading the exported files did not
invoke a frontier build; this eliminates the per-page repeated build, not the
later fresh semantic checks. Profiling overhead is present; no whole-workload
percentage speedup follows from these small-fixture timings.

The actual `daily_use.consume_pages` function was separately executed against a
real public export and genuine page-v2 acquisition. Both reconstructed the exact
same source/edge/known-hold inventory and preserved bank/feature/index/HEAD. Eight
corrupted/unsafe cases refused: page bytes, missing page, extra object, changed or
missing marker, unsafe directory mode, page symlink and hardlink. Its function
hash is retained in the [sanitized evidence](data/2026-11-07-checked-pages-development.json).
This exercised real function code via bounded AST loading without executing the
controller's top-level workload; it is not a mock or a full-controller pass.

[Independent Fable implementation review](2026-11-07-checked-pages-fable-review.md)
returned PASS with no blockers, actual `anthropic/claude-fable-5`, fallback=false,
native omp18.16.0. This is static technical review plus the receipts it inspected,
not full stress/platform acceptance. The review's phrase “no marker on any
failure” is narrower than the actual contract: an error after marker creation
can leave a marker, but no successful operation receipt. Marker presence alone
never authorizes consumption or publication; guides require exact receipt pins.

## Retained apparatus failures and integrated blocker

The first full-controller pilot stopped before workload because the APFS-created
worktree contained ignored bytecode files and failed the complete portable source
inventory. No source guard was weakened or cached file deleted. A fresh
`git clone --no-local` of the exact candidate passed all3351tracked/216portable
source-byte/mode checks.

That fresh six-session/128-seed export pilot then failed during authority rebind,
before its first capture: `scope_binding.apply` reported “Unknown or unsafe
recovery marker; retain it for reviewed recovery.” A diagnostic captured the
actual ordinary0600`transaction-marker-v1`:13,828bytes,
SHA256`934a7a80447e411a6a97418b91a0d6c096fb50e49efb4a8b34ac5a02456d26c8`.
The integrated `recovery_frontier.marker_bytes` rejects every marker over4096bytes,
including this exactly owned transaction carrying complete observed hashes.
The new export branch is not reached. The #244 owner was asked to fix the owned
marker contract without enlarging unknown-marker trust or shrinking the fixture.
This blocks integrated controller qualification, not the demonstrated small-bank
export/publication/refresh behavior. No larger/full run or Linux pass is claimed.

The first consumer-only diagnostic used a lexical `/tmp` output path while the
producer correctly returned its canonical `/private/tmp` path; the consumer's
exact receipt-path check refused. The corrected fixture uses resolved paths,
matching `World.inside` in the real controller; its retained run passes. No
product path check was relaxed.

Raw receipts/streams/profiles and fixture marker are private under
`/tmp/agent-memory-v15-readiness/209-*`; they can contain fixture tokens and
withheld bodies. Only sanitized data are committed. Follow-up qualification
requires the reviewed #244 fix on the integrated source, unchanged controller
faults and no invented speedup from overlapping invocation durations.
