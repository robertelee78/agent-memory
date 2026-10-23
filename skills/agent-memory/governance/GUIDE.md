# Correct and forget project learning

At natural turn boundaries the session MUST automatically prune incorrect or
irrelevant guidance when stronger evidence establishes the correction or loss
of applicability. Age alone MUST NOT justify pruning. The session MUST use the
[source authority rules](../authority/GUIDE.md), [security](../security/GUIDE.md)
and [shared lifecycle model](../model/GUIDE.md). It MUST resolve this repository
and the detecting developer through [identity](../identity/GUIDE.md).

## Choose the actual operation

| Operation | Behavior |
| --- | --- |
| `correct` | Qualified new source, same stable ID and higher version; preserve the old origin/body in history. |
| `supersede` | Link a currently qualified existing replacement; version that replacement and retain original source history. |
| `revoke` | Deactivate a disproved or no-longer-applicable claim without requiring a replacement. |
| `expire` | Deactivate only after the record's actual known `valid-until` boundary. |
| `retention` | Apply an actual established retention policy, separately from elapsed age or expiry. |
| `delete` | Explicit operator deletion of whole identities and authorized copies; leave only safe terminal tombstones for them. |
| `reactivate` | Explicit new evidence and a newly stated current validity period; higher version, never a deleted identity. |

Routine qualified correction/pruning MUST NOT require per-record operator
approval. Ambiguous evidence MUST stay pending with the detecting developer.
A claim obsolete on main MAY still apply to older branches/environments. The
session MUST retain that narrower guidance before removing broader applicability;
it MUST NOT use current-main evidence to revoke all older contexts.

A scope correction MAY retain every old condition and add a known restriction.
When older contexts remain valid, it MUST retain the exact old body; compatible
wording tightening can follow through separate consolidation.
For example, first retain a still-valid older-branch lesson under that branch;
then separately admit genuinely new main-branch learning with its own evidence.
For this exact-body narrowing, the comparison classification is
`scope-correction`, which returns `governance-required`. It is not ordinary
equivalence or a claim that the two compatible scoped statements contradict.
Admission and consolidation MUST leave that disposition pending for governance.
The helper MUST NOT remove/change an existing condition, widen scope or change
type to make a correction fit. Distinct identities/conditions need explicit
semantic reconciliation; unknown applicability MUST NOT become known by assertion
of confidence. Procedures require their separate explicit promotion review.

## Read existing learning and actual evidence

Before a change, the session MUST read the bounded router, all present ordinary
context files, and plausible typed matches with their conditions, source and
provenance. It MUST search the same concept and alternate wording, including
inactive history. It MUST inspect copied or paraphrased guidance before declaring
its source removed. A list of hashes records the observation; it does not prove
the agent actually read or understood every source.

For `correct`/`reactivate`, follow [admission](../admission/GUIDE.md) to prepare an
eligible capture under the target's stable ID (candidate version 1); the helper
computes the higher canonical version. The capture MUST name the actual correcting
developer/session and retain actual evidence. Follow
[comparison](../comparison/GUIDE.md) with that exact captured candidate.

For `supersede`, compare the complete unchanged metadata/body of the existing
replacement. Read the old target too, and classify the replacement as equivalent
to itself. It MUST qualify under current recall, with the same type, scope,
conditions, applicability, valid-time bounds, trust and visibility as the target.
A different applicability group MUST remain separate instead of being silently
collapsed. Adding its supersession relation creates a higher replacement version;
it does not silently mutate an earlier historical version.

Other unresolved conflicting identities or uncertain judgments MUST stay pending;
confirmed stronger evidence may resolve only the reviewed target's versions.
Other equivalent current identities require consolidation first. A conflict from
another developer SHOULD prompt recommended clarification with that developer;
the helper sends no messages. Current code/ADR/work-record contradictions MAY
require a focused PM Q&A; a new landing order does not establish truth.

## Prepare the exact request

The portable `govern_memory.py preview` reads one UTF8 JSON request, at most
256 KiB, with exactly these fields:

| Field | Required value |
| --- | --- |
| `project_id` | Actual resolved bank UUID. |
| `operation` | One operation from the table above. |
| `target` | Actual `{path,id,version,sha256}` of the canonical standalone target. |
| `authority` | The explicit source assessment below. |
| `comparison` | `{request,proof}` from fresh comparison for correction, supersession or reactivation; otherwise `null`. |
| `capture` | Eligible admission capture/response for correction/reactivation; otherwise `null`. |
| `replacement` | Actual `{path,id,version,sha256}` for an existing supersession replacement; otherwise `null`. |
| `delete_ids` | Explicitly authorized whole record IDs for deletion; otherwise `[]`. |
| `context_review` | Current SHA256 for `MEMORY.md` and every present ordinary context file. |
| `context_edits` | Subset of reviewed ordinary context paths mapped to complete corrected text, or `{}`. |
| `context` | Actual applicability context using the retrieval contract. |

The authority object contains exactly `source_class`, `repository_evidence`,
`evidence`, `reason`, `confirmed`, `strength`, `basis`, `impact`, `review` and
`reason_class`. `source_class` uses the security source classes; `evidence` is a
bounded nonempty list of actual citations and `reason` a safe concise one-line explanation.
`repository_evidence` is `null` or actual `{path,sha256}` outside memory/private
metadata. Except for mechanically observed expiry, a non-user source requires that observed supporting repository file;
assistant/tool/external sources also require an explicit nonempty `review`.
For correction/reactivation these source fields MUST match the actual capture.
A remembered statement labeled user MUST NOT be relabeled as a current operator
instruction or permission.

`confirmed` is an actual boolean. `strength` is `established` or `ambiguous`.
`basis` is `stronger-evidence`, `loss-of-applicability`, `time-boundary`, `policy`,
`explicit-deletion` or `ambiguous`. `impact` is `all-target-conditions`,
`narrower-contexts-remain` or `unknown`. The session MUST assess these from actual
evidence rather than fill favorable defaults. Unknown/ambiguous evidence or
impact MUST defer. Only a correction that actually narrows retained conditions
may proceed while narrower contexts remain. Explicit deletion also requires
whole-target applicability; a current-context-only request MUST NOT globally
create a tombstone. Expiry uses the observed clock and existing validity bound,
not a fabricated user instruction or a record's age.

`reason_class` MUST be `null` except for deletion, where it is a safe tombstone
class (`user-request`, `incorrect`, `obsolete`, `sensitive` or `retention`). It
MUST NOT contain original text, title, evidence, source hash or a free-form reason.
Semantic source strength and confirmation remain agent assertions; hashing a
source is not proof of its truth or authentication of its author.

With `SKILL_DIR` resolved, keep requests/proposals outside the canonical bank and
sharing delta:

```sh
python3 -I "$SKILL_DIR/scripts/govern_memory.py" preview --repo /path/to/repo \
  --request /private/path/governance-request.json > /private/path/governance.json
python3 -I "$SKILL_DIR/scripts/govern_memory.py" apply \
  --proposal /private/path/governance.json
```

Requests are at most 256 KiB and apply proposals 32 MiB. Changed canonical
files remain at most 16 KiB each; the common security batch is at most 64 files
and 64 KiB. Root/index capacities and complete evidence still apply. Exhausted
bounds MUST refuse rather than truncate provenance; the session MUST organize or
split independently authorized work and review again. These bounds are not a
large-bank latency guarantee; validation and hashing still read the whole bank.

The session MUST apply only a non-null proposal. A pending disposition identifies
clarification, narrowing, consolidation or explicit deletion scope still needed.
It MUST NOT invent stronger evidence to bypass one. Complete source/meaning review
remains the session's job; the helper verifies exact sources and mechanical guards.

## Terminal deletion and derived views

Deletion MUST come from explicit current operator authority. The session MUST
identify authorized independent duplicate copies as well as linked derivatives;
semantic equivalence is not inferred from UUID links. `delete_ids` may include
those actually authorized same-project identities. The helper computes the
transitive `derived-from` and owned-source closure (at most 64 IDs); missing authorization for
additional canonical derivatives returns required IDs and no mutation. If those
records also preserve independent knowledge outside the operator's requested
scope, the session MUST seek clarification before extending deletion.

Empty topic-index chains may be removed only when they contain generated routing
boilerplate. Custom index introductions MUST be preserved through the existing
file workflow before removing their last route; the helper refuses to discard
unrelated prose. Nonempty index introductions and the root introduction survive.

Consolidation retains each original capture ID in
`history/sources/<capture-id>/<source-id>.md`, alongside its required source
reference. Deletion includes both that retained observation and its original
capture ID, even if the capture never became an independent active record. Their
terminal markers prevent a saved capture from being admitted again. An older or
malformed source path without a verified capture role returns
`provenance-identity-review-required`; the session MUST preserve it and resolve
the actual original capture identity before moving it into the structured path.
It MUST NOT guess from reference order or silently discard source provenance.

The operation removes every version and event for deleted identities and creates
only their five-field empty-body terminal markers. Existing terminal markers
remain unchanged. A retained independent correction need not be deleted merely
because it superseded an erased wrong claim; its redundant terminal pointer is
removed while its body, own evidence and origins remain intact. Tombstones now
supply terminal invalidation. Original assertions MUST NOT be copied into new
shared deletion events, reasons or markers.

The session MUST use reviewed `context_edits` to remove copied assertions from
ordinary prose while preserving unrelated content. Exact retained body copies
in reviewed context/router cause refusal. If a router introduction or other
unexpected prose itself contains the claim, the session MUST first use its
existing governed file workflow to clean that copy and review again. This check
does not prove that every paraphrase was found.

Owned projections are body-free references bound to complete canonical hashes.
Any mutation invalidates their former observation; consumers MUST reject stale
projections and rebuild using canonical lifecycle/tombstone guards. Old cached
payloads and consolidation proposals MUST NOT restore deleted identities or be
relabeled as new learning. External copies and Git history are separate retention
boundaries; this operation does not rewrite Git objects or erase arbitrary stores.

## Verify, recover and continue

Apply requires `verified: true`, `outcome: finished`, and
`governance_verified: true`. The helper checks the actual bank, source evidence,
Git context, complete recomputed delta and applicability under the common lock
and during readback. It MUST preserve unrelated code, index and HEAD. A new
corrected version retains the stable ID and actual new source origin, with the
previous origin/body in history and its original link base recorded. New evidence
is required; a new operation UUID alone is not new evidence. Reactivation retains
the prior inactive state in history and needs an explicitly current new period.

Follow [write recovery](../writes/GUIDE.md) after interruption. Before a complete
verified finished receipt, finish MUST refuse: roll back the exact partial
operation, inspect sources and compare again. After the exact complete receipt,
finish only cleans the recorded outcome without replaying bytes or requiring new
semantic evidence. Unknown or changed receipts/journals and external bank edits
require inspection. Exact replay preserves later corrections; `still_current:
false` means historical success only, not current guidance.

Routine success SHOULD stay quiet. Conflicts, failures and consequential
corrections MUST surface promptly at a boundary. Exit 0 MAY mean pending;
exit 1 means refused or uncertain state, and an apply failure MUST NOT imply
no canonical writes. Exit 2 means invalid arguments. This local operation does
not publish, refresh main or advance working code. Sharing MUST remain pending
until the repository's actual memory-only exchange verifies it.
