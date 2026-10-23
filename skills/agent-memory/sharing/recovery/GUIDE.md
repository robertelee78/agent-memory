---
kind: operation
domain: sharing
---

# Recover shared observations, lost responses and merge baselines

## Use when

This guide covers a fresh remote observation, lost-response readback and the
mechanical three-way merge used by the sharing operations. A local observation,
a mechanically compatible merge and an accepted semantic review are distinct
facts. The helper MUST NOT be treated as permission to publish or as evidence
that feature code matches the authority branch.

## Required context

A session using this leaf MUST first load and follow the [sharing domain](../../sharing/GUIDE.md). It MUST open the [shared model](../../model/GUIDE.md) only when an unresolved semantic question requires it.

A session MUST retain useful local capture while remote exchange is pending.
It MUST NOT claim freshness after a failed/offline observation. It SHOULD surface
the failure at a natural boundary and continue independent local work.

## Inputs and preconditions

The session MUST inventory before retrying a push/merge. The replay request is
exactly `{ "base": "FULL_SHA", "candidate": "FULL_SHA" }`. Both MUST name actual
locally available commits. The candidate MUST have exactly the previous observed
authority base as its sole parent and change only canonical memory paths.

The complete three snapshots are bounded: 256 files, 16 KiB per file and 1 MiB
total. Publication readback inspects at most 128 intervening commits. Exceeded
bounds MUST require review, never silently truncate evidence or claim absence.
This foundation does not bypass publication checks or implement automatic
semantic approval, landing or boundary invocation; those operations have their
own contracts.

## Observe

With `SKILL_DIR` resolved to this package:

```sh
python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" inspect --repo /path/to/repo
python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" inspect --repo /path/to/repo --fetch
```

Untracked canonical learning does not prevent this read-only remote inventory;
it is still not proof of semantic admission or publication.

An authority prefix that has never contained a bank reports actual absence and
`shared_bank_present: false`. The session MUST use the first-publication route
with its existing bound local bank, not create a second starter. An absent
previously occupied bank MUST require historical inspection rather than erase
its former invalidation history.

The first command is local only and reports pending/stale sharing. The second
checks the actual selected server's default branch, fetches its immutable head
into a unique disposable Git ref, reads its complete bounded canonical bank,
and checks the server again. A competing advance makes the observation pending.
The temporary ref is removed; no branch, index, working file or `FETCH_HEAD`
is replaced. Git objects may be fetched and ordinary credential transport may
run. The selected remote's permissions remain authoritative. A completed fetch reports
Git metadata writes separately from `canonical_writes_performed: false`. A
failed attempt reports unknown (`null`) metadata-write status; it MUST NOT claim
that no Git metadata changed.

A fresh observation MUST be used only for its recorded head and time. A later
mutation MUST recheck that head and its current local source/scope pins. Local
remote-tracking refs and a cached JSON result MUST NOT establish server freshness.
Losing the temporary ref, optional cursor or retry state loses no Markdown.
Unknown locks and transaction markers MUST remain for ordinary write recovery.

## Plan

A session MUST choose the actual observed case: inventory a possibly completed publication before any retry, or reconstruct a three-way memory baseline from preserved sources. It MUST keep feature code/index/history and unpublished memory separate from these readback/planning primitives.

## Apply

```sh
python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" replay --repo /path/to/repo --request /private/replay.json
```

For a mechanical three-way baseline/candidate inspection, the session MUST use the appropriate reconcile/base branch below.

```sh
python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" reconcile --repo /path/to/repo
python3 -I "$SKILL_DIR/scripts/sharing_recovery.py" reconcile --repo /path/to/repo --base FULL_SHA
```

## Read back

The helper fetches current authority and checks the bounded intervening history
for an actual first-parent authority tree containing the complete intended memory
delta. Merely reaching a proposed side commit MUST NOT count as publication: a
merge resolution may have discarded its memory. The previous observed base MUST
be present in the bounded authority mainline; a history flip or unproved base
requires inspection. It can
recognize squash/recreated commits from their bytes without relying on commit
messages or a locally remembered success flag. `intended_delta_landed: true`
means the intended bytes were observed on authority; `intended_delta_current`
separately says whether they still match the latest observed head. A later
correction MUST NOT be undone merely to make an older replay receipt current.
A witnessed publication MUST NOT be pushed again. An unwitnessed delta requires
fresh reconciliation and repository rule checks, not an unconditional retry.
Rewritten/diverged authority or exceeded history bounds require review.

The helper compares previously shared memory, current local canonical memory,
and freshly observed authority. With no cursor, the unique actual Git merge
base reconstructs the comparison baseline; ambiguous/unrelated histories MUST
be reviewed. The explicit base MUST be reachable from current authority. It need not be an
ancestor of the feature HEAD: a previous memory-only import MUST NOT require
merging authority code or changing feature ancestry. Local HEAD remains pinned
and a changed HEAD requires a new comparison. Losing a
cursor MUST NOT justify replacing the whole bank with an authority snapshot.

## Verify

The result contains a complete validated candidate and exact local/source
hashes. Independent file edits combine; identical edits occur once. Structured
route maps combine with their introductions and omitted-context reasons
preserved. Concurrent incompatible edits remain pending for the detecting
session's semantic review. Different files can still disagree: every candidate
requires the following semantic reconciliation operation, even when Git found
no textual conflict. A candidate MUST NOT be automatically applied merely
because format validation passed.

Tombstones are monotonic across all three snapshots. Older versions and locally
edited bodies for erased identities are excluded from the candidate. An older
branch MUST NOT remove a terminal guard or restore guidance from that identity.
An inactive stable version MUST NOT become active through an older working
copy; actual reactivation requires a higher version and fresh lifecycle review.
A newly found derived observation of an erased identity requires explicit
closure/erasure review. Conflicting tombstone authorities, ambiguous lifecycle
changes, unsupported shape changes and custom index introductions that would be
lost MUST remain pending rather than select a winner by time or landing order.

## Outcomes

After semantic approval, the calling sharing operation MUST use the existing
recoverable multi-file write protocol with current source and Git pins. The
mechanical helper itself does not write canonical memory or resolve Git's index.
A bank already damaged by an ordinary merge MUST be restored/reviewed from the
three preserved inputs before mediated use; conflict markers are not a valid
bank. Once imported memory is committed and normal code integration runs, the
next comparison recognizes the same landed memory without duplication/reversal.
The session MUST preserve unrelated index entries, code changes and branch
ancestry. A refresh MUST NOT imply that code from authority was imported.

## Failure handling

If authority, history, scope, snapshot bounds or source observation cannot be verified, the session MUST retain useful local work and report sharing pending/stale. It MUST NOT infer absence from an incomplete inventory, treat a discarded side parent as a landed witness or overwrite local memory from a directory snapshot.

## Diagnostic fallback

If normal readback cannot establish what landed or the correct base, a session MAY inspect bounded actual first-parent history, canonical trees and preserved proposals through read-only Git/file tools. It MUST keep uncertain publication unverified and complete fresh semantic review before a corresponding write or exchange.
