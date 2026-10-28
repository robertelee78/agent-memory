# Restricted-push memory publication RCA

Date: 2026-10-28. Planning source: [ADR-0022](../adr/0022-restricted-push-publication.md).
Implementation: `53b3a27a4c2a12d5b8937852f3b8c8f57cdab4cb`.

The operator reported a local reachability decision awaiting publication because
push actors were restricted, followed by a retained Codex assessment notice.
Read-only API inspection found classic restrictions and an explicitly allowed
current Mac user. That does not identify the reported session's transport or API
credential, nor establish successful publication from another machine.

## Cause and correction

`memory_publication.policy` previously appended `restricted-push-actors` for
**every** non-null classic restrictions object. It never inspected the actor or
allowed-user list. The result was a conservative unsupported-policy refusal,
even when an explicit ordinary grant could be proven; it was not a witnessed
GitHub push rejection. Existing policy tests covered unknown rules, required
checks and App identities, but had no positive restricted-user publication case.

The correction reads `/user` and the complete allowed-user inventory within four
100-entry pages. Stable user ID plus case-insensitive login must match; an admin
or general write permission is insufficient. Unknown/malformed identities,
duplicate IDs, non-200 read responses, incomplete pagination and unsupported
team/App-only eligibility remain pending with a repair description. Validated
identity/allowlist evidence participates in the pre-push policy fingerprint.
The existing protected PR/review/check/merge-object path and normal Git push
remain intact. A refused transport names possible API/Git credential mismatch;
a successful API eligibility observation is never called a successful push.

Independent code review reproduced a further reader defect: the shared API
reader accepted HTTP 201 for GET. This was corrected to GET 200 while preserving
successful POST 200/201; final tests include both actor-201 and later-page-201
refusals. The initial implementation review was BLOCK, not retrospectively a pass.

The second notice is a separate, honest unfinished-assessment state after the
bounded Stop continuation cap. No local admission is reclassified as publication
or no-learning. The original consumer source observations remain retained; this
correction does not fabricate their completion or silently replay their learning.

## Qualification boundaries

All mutation tests use copied public helpers, disposable real Git repositories,
and an actual pre-receive restriction/review/check guard. No live consumer bank,
code, index, branch or GitHub protection was changed. A read-only live policy
observation with the exact final runtime recognized an explicitly listed user and
retained every configured required status/check identity. It proves that API
policy case only, not another machine's credentials or an actual consumer landing.

The initial actor run (5 pass, 253.489 seconds) overlapped last edits and is retained
as iteration evidence, not bound to final source. Existing policy, pending and
recovery regressions passed 9 tests total in separate bounded workers; their
execution overlapped the GET-status correction, which leaves their GET-200 and
POST fixtures unchanged. The exact frozen final actor run is reported separately.
There were no broad remote CI or model trials for this shared Python correction.

Detailed [offline regression receipts](data/2026-10-28-restricted-push/regressions.json),
[initial iteration receipt](data/2026-10-28-restricted-push/initial-overlapping-run.json),
and [redacted live read-only observation](data/2026-10-28-restricted-push/live-policy-redacted.json)
retain their actual scope. Signed release delivery and consumer adoption remain
separate from source integration.

## Frozen final result

All five actor tests passed on exact runtime source
`53b3a27a4c2a12d5b8937852f3b8c8f57cdab4cb` in 263.398 seconds,
with zero failures, errors or skips. The tests cover successful reviewed landing,
12 negative identity/inventory variants, complete second-page matching, five
incomplete pagination variants, three immediate pre-push policy changes, and
actual Git-transport actor rejection while preserving local state.
The new suite is registered in the contract gate; registry validation passed.
The [final receipt](data/2026-10-28-restricted-push/final-actor-tests.json) is separate
from the earlier overlapping iteration. No further runtime changes followed it.
