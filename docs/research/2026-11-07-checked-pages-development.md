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

## Historical apparatus failures and integrated blocker

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
At that point this blocked integrated controller qualification, not the
demonstrated small-bank export/publication/refresh behavior.

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

## Integrated controller qualification after marker correction

The #244 integration owner delegated the narrow exact-owned-marker correction.
Both `_observe_index` and `require_scope` now preserve their existing owned byte
witness; unknown markers and recovery sentinels keep the4096-byte bound. The
first correction covered only the former; pilot03 reached admission and exposed
the second missing caller. Both failures remain evidence. Amended implementation
`227e3eab252905197e4e4fb3c789af0b8c8eb7a1` passes actual48-record admission
direct apply and durable-receipt interrupted recover-finish, exact prior learning
and feature/index/HEAD preserved, and14marker/frontier contract methods.

Two subsequent actual public-controller pilots completed, not just unit tests:

| Pilot | Composed source | Controller seconds | Public commands | Timeouts |
| --- | --- | ---: | ---: | ---: |
| 04 | `69df2f36317581e541914002c73210d6464cfcd4` | 3362.224 | 421 | 0 |
| 05 | `1be9ad0a5b67da637c5f1924831f6b52c9b2cbe0` | 3087.743 | 421 | 0 |

Each retained six sessions,128seeds,two rounds,four workers,three independent
clones,two developers,19captured lessons, all declared semantic/recovery controls,
exact learning/provenance/conditions and expected feature state. Complete source
bytes/modes match before/after (pilot04:3366tracked/216portable;
pilot05:3368tracked/216portable). Artifact hashes and explicit outcome flags are
in the adjacent sanitized JSON; raw fixture receipts stay private.

Pilot05 includes separately integrated read-path reuse and mixed-capability
consumer changes. These are different product revisions on the normally busy
host, not a matched page-fetch A/B comparison or causal/statistical speedup.
Neither small cohort proves practical24-session/10,000-seed runtime. Both retain
`full_daily_use_v1_acceptance:false` and `required_fault_matrix_complete:false`;
no Linux or native-harness qualification follows from controller labels.

The marker correction has scoped Opus source approval reported by the integration
owner. Full admission-suite verification is not green:16of17methods passed and
the existing10-second final-readback ready-file rendezvous failed. A separate
actual diagnostic reaches the boundary, rejects changed repository evidence,
preserves recovery state and restores exact bank/feature state through rollback;
the historical rendezvous-failure cause remains unconfirmed. No guard/assertion
was relaxed. The complete marker verification record is retained in evidence-only
commit `545d732` on `fix/244-owned-large-marker`.

#209 remains Executing/At risk, not accepted or delivered. Its obsolete marker
blocker was cleared through a verified AWA reconciliation. Final source freeze
awaits the integration owner's remaining native OMP FIFO/manual-ancestor/mixed-cost
corrections and reviews; full numeric qualification protocol is still pending.
The shared-host performance lane was explicitly handed to #206 after pilot05
terminated. No #209 benchmark remains active; do not restart the entire workload
for each intermediate integration edit.

