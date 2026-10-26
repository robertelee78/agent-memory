# Newly shared topic indexes could not accept the next compatible update

Work: #99, `ADR-0009:daily-use-reliability`.

The first real two-round daily-use controller used three independent clones,
two developer identities and three separate feature worktrees. Every session
admitted learning through public capture/discovery/comparison/admission, all
three publication plans preceded the first push, stale plans reconciled, and
each session imported and recalled a peer's exact record with its feature state
unchanged. The next round admitted its three new lessons, then semantic review
refused. This happened on macOS and Linux against combined `d71c424`.

The retained macOS run is
`/tmp/agent-memory-v003/daily-use-v1-smoke-3-128-v6/`; the Linux run is
`/tmp/agent-memory-v003/daily-use-v1-linux-smoke-3-128-v6/`. Their controller,
source checks, raw input/output blobs, failure and phase events distinguish
successful first-round exchange from the failed second round. This small
development run does not replace either required full workload.

## Cause

The feature branch's Git ancestry intentionally had not advanced. Its actual
common Git base predated the newly shared topic indexes. The local and landed
banks now contained an index at the same path, with the same introduction and
header but different compatible route additions. The merge primitive treated
any index missing on any side as one indivisible file and refused both versions,
before its existing structured route-table merge could run. A normal imported
map therefore could not accept another independent child through this path.

## Repair and limits

When an index is absent at the **actual base** but present on both other sides,
merge its checked header fields and route tables with that absence retained as
the empty map. Require exact agreement on the introduction; do not invent an
empty historical prose baseline. Competing values for the same route or header
still refuse. Observation timestamps retain the existing metadata rule; they
never select a claim. The original typed payload merge, deletion guards,
invalidation checks, project validation and final complete bank checks remain.

Public controls reproduce the predecessor refusal and exercise independent
route additions, different new introductions and competing captions for one
target. A compatible/equivalent agent label cannot choose among unsupported
prose or route replacements. The full workload must be rerun on the repaired
source and continue through useful peer recall; a focused merge check alone
does not close #99.
