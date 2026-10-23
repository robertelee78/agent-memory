# Consolidate qualified duplicate learning

At a natural turn boundary, after foreground work, the session MUST automatically
combine equivalent learning and tighten wording when current evidence supports
preserving its meaning. This needs no daemon or per-record approval. The session
MUST follow the [comparison guide](../comparison/GUIDE.md), inspect all plausible
sources, and preserve distinct conditions, evidence and developer/session origins.
A semantic judgment is an agent assertion, never a mechanical proof of truth.

## Prepare one source-bound group

The packaged `consolidate_learning.py` takes a JSON input with exactly:

| Field | Required value |
| --- | --- |
| `comparison_request` | The exact request used for the fresh comparison. |
| `comparison_proof` | The proof returned by review, or that review response. |
| `capture` | A qualified admission capture or response, or `null` for tightening existing learning. |
| `context` | Actual applicability context using the [retrieval fields](../retrieval/GUIDE.md). |

For existing wording alone, the candidate metadata MUST be the complete unchanged
metadata of the existing target chosen to retain its stable ID; only the body
proposes new wording. For a new duplicate observation, the session MUST first
follow [admission](../admission/GUIDE.md) to capture its actual evidence and origin,
then compare that exact captured candidate. An unconfirmed or procedural capture
MUST remain pending its separate review or explicit procedure promotion.

The helper qualifies every target through current recall before writing. Its
bounded group has one to eight typed targets with the same kind, scope,
conditions, applicability, valid-time bounds, trust and visibility. Unknown,
mismatched, expired, revoked or deleted sources MUST NOT be made current by
consolidation. Distinct conditions MUST remain separate; an unavailable source
MUST NOT be treated as absent. Other-bank or stale comparison evidence refuses.

With `SKILL_DIR` resolved, save evidence and proposals outside canonical memory:

```sh
python3 -I "$SKILL_DIR/scripts/consolidate_learning.py" preview --repo /path/to/repo \
  --input /private/path/maintenance-input.json > /private/path/maintenance.json
python3 -I "$SKILL_DIR/scripts/consolidate_learning.py" apply \
  --proposal /private/path/maintenance.json
```

A preview MUST contain a non-null `proposal` before apply. `no-change` means
nothing needs writing; repeat execution converges without another version or
event. A distinct higher-order idea returns `proposal-only`, not a new decision.
Contradictions or uncertain meaning MUST go to the detecting developer/session.
If another developer originated a conflicting claim, the session SHOULD recommend
clarification with that developer. The helper sends no messages. A real unresolved
code, approved-intent or work-record conflict MAY require a focused PM Q&A.

## Preserve provenance and history

A successful consolidation retains one existing stable ID and creates a higher
active version. It preserves original identity and applicable metadata, unions
evidence and derivation links, and emits explicit supersession and version events.
The previous exact primary body and origin remain in a routed historical version.
Other equivalent records become inactive with their original bodies, conditions,
evidence and origins intact. A newly captured duplicate retains its complete
source metadata and body in an inactive provenance record; it does not become
another independent active lesson. These records are historical evidence, never
current guidance or a route around later revocation or terminal deletion.

Captured provenance MUST retain the original capture ID in the structured path
`history/sources/<capture-id>/<source-id>.md` and its required derivation link.
That identity also locates the source and owning lesson after inactive-body
compaction. [Governed erasure](../governance/GUIDE.md) includes the original
capture ID, so an old saved capture cannot be admitted again after deletion.
Existing source identity/conditions/provenance MUST NOT be discarded or guessed
when repairing an older source path. `governance-required` scope narrowing MUST
remain pending for governance; consolidation MUST NOT preserve the old broader
scope while pretending that it narrowed the learning.

The helper retains old correction/supersession links so tightening does not
reactivate an earlier disproved claim. It refuses a revoked, expired or deleted
provenance source instead of silently rehabilitating it. The session MUST use
the separate governed correction/forgetting workflow for changes in meaning,
requalification, pruning or deletion. Procedure promotion remains separate.

Router and topic-index updates preserve their existing introductions and routing
conditions. Full files and source metadata MUST fit the existing record/router
bounds. Maintenance MUST stop before writing if complete provenance exceeds a
bound; the session MUST reorganize into useful bounded groups without discarding
evidence. Readers and operations still scale with the complete bank.

## Verify, recover and continue quietly

Successful apply requires `verified: true`, `outcome: finished`, and
`consolidation_verified: true`. The exact observed source bank, Git context,
current applicability and optional capture evidence are checked before the write,
under the common lock, and again during transactional readback. The helper MUST
NOT change unrelated source files, the Git index, HEAD or feature ancestry.

A repeat of the same operation reads its existing receipt without overwriting
newer edits. `still_current: false` describes historical success only; the session
MUST qualify current sources before using the lesson. A different plan needs a
fresh comparison and operation ID. A local receipt establishes no publication
or latest-main freshness; actual exchange MUST compare against newly landed
memory and follow repository rules.

An interruption MUST preserve its journal, marker and private receipt. Follow
[write recovery](../writes/GUIDE.md): recover the dead owned lock first. Before a
verified finished receipt, transaction finish MUST refuse; roll back the exact
partial operation and compare again. After a complete verified receipt, finish
only cleans that exact committed outcome, without fresh semantic evidence or
replaying bytes. Unknown receipts, missing journals or external edits require
inspection. After marker removal, exact replay preserves later memory changes.

Routine success SHOULD stay quiet. Conflicts, failures and consequential
corrections MUST surface promptly at the boundary, without interrupting unrelated
foreground work. Exit 0 MAY mean pending or no change; exit 1 means refusal or an
uncertain outcome and requires inspection before retry; exit 2 means invalid
arguments. An apply failure MUST NOT be assumed to mean no canonical writes.
The helper runs no daemon, executes no remembered commands, and grants no
repository policy exception.
