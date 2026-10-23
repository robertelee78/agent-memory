# Capture and admit durable project learning

At a natural turn boundary, a session MUST consider durable discoveries, verified
fixes, useful failed approaches, user corrections and confirmed project decisions.
It MUST automatically capture qualifying learning without seeking approval for
each routine record. It MUST keep the unit small enough to change a future
session's behavior under explicit conditions. Current operator instructions and
the [source authority rules](../authority/GUIDE.md) remain authoritative.

A session MUST NOT turn assistant proposals or unanswered recommendations into
confirmed decisions. It MUST NOT accumulate raw transcripts, easily recovered
source facts or copies of AWA/Vox work state. Transient observations without
future utility SHOULD be discarded. Unknown applicability MUST stay unknown;
evidence for a conditional lesson does not make it universally true.

## Prepare a small capture

The packaged `admit_learning.py` accepts a UTF8 JSON request with exactly:

| Field | Meaning |
| --- | --- |
| `project_id` | Actual resolved repository UUID. |
| `candidate` | `{metadata, body}` in the [record format](../records/GUIDE.md), status `candidate`, new random ID, version 1. |
| `source_class` | `user`, `verified-repository`, `tool`, `assistant` or `external`; MUST match `metadata.source.kind`. |
| `repository_evidence` | `null`, or actual current local `{path, sha256}` outside memory/private metadata. |
| `assessment` | The explicit judgment below, made after reading the source. |

The assessment has exactly these fields:

```json
{
  "category": "user-correction",
  "utility": "Prevent repetition of the evidenced mistake under these conditions.",
  "durable": true,
  "evidence_confidence": "confirmed",
  "evidence_reason": "The current operator explicitly confirmed this project correction.",
  "confirmed": true,
  "source_redundant": false,
  "work_state_copy": false,
  "review": null
}
```

`category` MUST be `durable-discovery`, `verified-fix`, `failed-approach`,
`user-correction` or `confirmed-decision`. `evidence_confidence` MUST be
`confirmed`, `inferred` or `unconfirmed`. The three reason/review strings are
bounded to 1 KiB each; `review` MAY be null. The full capture request is bounded
to 16 KiB and the readable learning body to 4 KiB. The session MUST split larger
learning into distinct useful units rather than truncate evidence or conditions.

A session MUST fill the record template with actual project/developer IDs,
evidence, source actor, harness, session and applicability before capture. It
MUST NOT use template UUIDs or example conditions as observations. New capture
requires the actual detecting developer; existing authorship MUST be preserved
through consolidation instead of relabeled. Unmerged observations MUST retain
an actual current branch/revision condition or unknown applicability. This local
operation cannot establish default-branch freshness, so it refuses an originating
`default-branch` claim. Unknown code applicability MUST remain `unknown`.

`verified-repository` requires a canonical Git-visible source path and matching
SHA256, also named in record evidence. Assistant, tool and external sources
require both that actual supporting source and an explicit `review` assessment
before automatic admission. A verified tool result MAY use a retained bounded
result/report as its source; a bare assertion that a tool succeeded is insufficient.
The helper observes source bytes, not semantic truth or the identity of an author.
The session MUST NOT claim that hashing a report verifies its assertions.

With `SKILL_DIR` resolved, save inputs/results outside the canonical bank and
outside any sharing delta:

```sh
python3 -I "$SKILL_DIR/scripts/admit_learning.py" capture --repo /path/to/repo \
  --request /private/path/capture-request.json > /private/path/captured.json
```

`eligible` means the capture qualifies for fresh comparison, not admission.
`review-required` keeps an explicitly unconfirmed candidate outside current
guidance. `discard` creates no candidate artifact. `quarantine` withholds content
and produces no reusable capture artifact; the original input MUST be reviewed
and redacted privately before a new capture. The helper writes a private,
content-free policy audit, never a shared transcript or unsafe quarantine file.
Procedural candidates MUST stay pending the separate explicit promotion workflow;
routine admission MUST NOT promote them or create global/personal preferences.

## Compare, then preview and apply

A session MUST follow the [comparison guide](../comparison/GUIDE.md): use the
exact captured `{metadata, body}` in the comparison request, search the concept
and alternate wording, read plausible matches including their conditions,
evidence and provenance, and classify every match. The capture response and
comparison review response MAY be passed directly to the following commands:

```sh
python3 -I "$SKILL_DIR/scripts/admit_learning.py" preview --repo /path/to/repo \
  --capture /private/path/captured.json --request /private/path/comparison-request.json \
  --proof /private/path/comparison-review.json > /private/path/admission.json
python3 -I "$SKILL_DIR/scripts/admit_learning.py" apply \
  --proposal /private/path/admission.json
```

The preview MUST contain a non-null proposal before apply. Equivalent/refined
learning returns `update-existing` or `consolidate-existing` instead of creating
a duplicate identity. Inactive targets require lifecycle review. Contradictions
return `clarification-required` to the detecting developer; another author's
UUID prompts recommended clarification with that developer, never automatic
messaging. A session MUST retain that pending operation until a governed
consolidation/correction or clarification resolves it. It MUST NOT label a match
distinct merely to bypass an unavailable operation.

Admission rechecks capture, actual evidence, full-bank comparison and the exact
planned delta under the common bank lock. A newly arrived memory invalidates the
old proof. It creates one active, reviewed record with its stable identity,
conditions and origin, an admission event and router entries using the recoverable
multi-file protocol. The record retains utility, evidence assessment, source
review and a content hash reference for observed repository evidence. Trust is
`reviewed`; source hashing does not promote it to `verified`. Related source
facts, history and unrelated code MUST remain intact.

The baseline appends bounded root routes. If the router has no capacity, the
operation MUST stop before writing; a session MUST use the existing [topic-index
layout](../layout/GUIDE.md) to organize the bank and compare again. It MUST NOT
silently omit a route or drop an older lesson to make room.

## Outcomes, interruptions and sharing

An apply receipt requires `verified: true`, `outcome: finished` and
`admission_verified: true`. A retry with the exact operation ID reads its existing
outcome without replaying over newer edits. `still_current: false` means historical
success only; the session MUST inspect current memory before further use. A
different plan MUST use a new operation ID and fresh capture/comparison evidence.

An interrupted admission MUST remain unreadable behind its transaction marker.
The session MUST follow [lock recovery](../writes/GUIDE.md). Before a verified
completion receipt exists, it MUST use transaction `recover --mode rollback`
with the original journal: partial memory cannot support a fresh complete
comparison. Rollback MUST preserve unrelated edits and is allowed even when
supporting evidence changed. After rollback, the session MUST inspect evidence
and compare again before a new admission.

If the exact verified finished receipt exists, admission already reached its
commit point. Transaction `recover --mode finish` MUST clean up that recorded
outcome without replaying bytes or demanding fresh semantic/source evidence;
rollback MUST NOT undo it. Later evidence changes do not change that historical
receipt into a new admission. The current bank must still exactly match the
recorded completed bytes; external edits, unknown receipts or missing journals
MUST remain for explicit inspection. After a crash following marker removal,
retrying the original admission reads its receipt and cleans its leftover
journal without replacing newer memory. A session MUST still qualify current
evidence before using the historical lesson as present truth.

This operation does not commit, fetch, push, merge or establish latest-main
freshness. It MUST NOT report local admission as shared publication. Actual
publication MUST refresh and repeat comparison against latest landed memory,
following repository permissions and branch rules. Until verified exchange is
available, sharing MUST stay pending. Routine captures SHOULD remain quiet;
conflicts, failures and consequential corrections MUST surface promptly at a
boundary without interrupting foreground work.

Exit 0 means the requested stage completed and MAY report pending/quarantined
learning. Exit 1 means refused/unavailable/changed state or an uncertain outcome;
the session MUST inspect and preserve sources before retrying. Exit 2 means invalid
arguments. Captures and semantic judgments are agent assertions, not authentication
or a proof that every possible paraphrase was found.
