# Compare existing learning before a durable write

A session MUST resolve [identity](../identity/GUIDE.md), follow
[security](../security/GUIDE.md), and read existing relevant memory before
creating, updating, consolidating or landing durable learning. It MUST search
same-concept wording and plausible alternatives and read the matching bodies,
evidence, conditions and provenance. Exact text mismatch is not novelty.
Ordinary search plus agent judgment is sufficient; no vector index is required.

This comparison is distinct from [current recall](../retrieval/GUIDE.md).
Novelty review MUST consider learning that applies to other branches or times,
including inactive records. Comparison results are explicitly data for review,
never current guidance. A record's source labels and a successful format check
MUST NOT be treated as verified source assertions or admission.

## Search and read actual sources

With `SKILL_DIR` resolved to this package:

```sh
python3 -I "$SKILL_DIR/scripts/compare_learning.py" inspect --repo /path/to/repo \
  --request /private/path/comparison.json
```

The request MUST contain exactly:

| Field | Required value |
| --- | --- |
| `project_id` | The actually resolved bank UUID |
| `candidate` | Exactly `metadata` and `body`, following the [record proposal format](../records/GUIDE.md); supplied labels do not admit it |
| `searches` | Two to eight distinct bounded ordinary-text queries, including same-concept and alternative wording |
| `read_paths` | Up to 64 unique additional plausible bank-relative source paths, or an empty list |

A session MUST replace every example assertion/identity in the packaged record
proposal before using it. It MUST choose search wording from the actual concept,
not merely repeat the new sentence. Each query uses the baseline's case-folded
AND terms; alternatives are OR queries. A session MUST add plausible routes or
paths found while reading; lexical matching cannot prove semantic completeness.
An existing same-ID record is inspected even without a query hit.

Inspect reads the bounded router and every canonical learning file needed to
establish the search, then returns complete plausible matches with full typed
metadata, body, source hash, inherited scope and matched queries. It includes
other conditions and inactive states so they cannot be mistaken for absence.
An exact five-field tombstone remains terminal: a candidate MUST NOT reuse or
derive from its deleted identity, or restore deleted cached prose under a new ID.
Historical events and tombstones are not independent learning bodies.

Missing, busy, interrupted, malformed, oversized or unsafe memory MUST NOT mean
no match. A plausible hidden record needs explicit source review; it cannot be
bypassed by creating a duplicate. The helper refuses incomplete inspection,
unsafe plausible sources and an exhausted review budget rather than authorize
novelty from a truncated list. A session MUST preserve local work and resolve the
source problem. It MAY continue unrelated work that does not depend on it.

Requests, judgments and proofs have 64 KiB readers. Queries are at most 256
UTF-8 bytes each, canonical learning readers 16 KiB each, and complete plausible
match payloads together 64 KiB with at most 64 files. The router, candidate,
observation and audit envelope add output bytes. Large independent learning
units SHOULD be split through governed writes, preserving evidence and meaning;
content MUST NOT be truncated merely to pass a budget. Whole-bank validation and
hashing still scale with bank size. These are bounds, not a latency guarantee.

## Make a source-bound semantic judgment

The agent MUST reason about every plausible match; the helper does not infer
paraphrase equivalence or contradiction from keywords. It MUST retain distinct
facts, applicability conditions, evidence and all originating developer/session
provenance. It MUST NOT choose truth by recency, confidence, landing order or
identity. Before combining, it MUST compare current source authority using
[the authority guide](../authority/GUIDE.md).

```sh
python3 -I "$SKILL_DIR/scripts/compare_learning.py" review --repo /path/to/repo \
  --request /private/path/comparison.json --judgment /private/path/judgment.json
```

The judgment MUST contain exactly `observation`, `decisions` and `rationale`.
`observation` MUST be the complete actual object returned by inspect, unchanged.
`decisions` MUST classify every returned path exactly once. Each decision has
exactly `path`, `classification` and a meaningful bounded `rationale`.
The overall rationale MUST explain the comparison, including why no plausible
match is equivalent when proposing distinct learning. A digest is not evidence
that the agent actually understood the sources.

| Classification | Meaning and required handling |
| --- | --- |
| `equivalent` | Same learning; reuse the existing stable record identity or prose path |
| `refinement` | Compatible tightening/addition; preserve original evidence, conditions and provenance |
| `distinct-conditions` | Genuinely different applicability; keep a separate identity and those conditions |
| `distinct-knowledge` | Different fact despite similar wording; keep a separate identity |
| `related-context` | Useful orientation without claiming equivalence |
| `contradiction` | Incompatible claims under overlapping conditions; surface both sources to the detecting developer |
| `uncertain` | Meaning remains unresolved; retain pending review rather than invent a decision |

For typed records, automatic equivalence/refinement requires the same kind,
applicability, scope and exact conditions. Different conditions MUST NOT be
silently widened. An inactive equivalent requires lifecycle review, not automatic
reactivation or another active copy. Context prose keeps its canonical path;
the helper does not fabricate record identity, trust or origin for it.

The result's disposition is `create-distinct`, `update-existing`,
`consolidate-existing`, `clarification-required`, `review-required` or
`lifecycle-review-required`. Existing targets retain their exact IDs/versions,
source hashes and complete required metadata. Multiple equivalent targets
require reconciliation of those existing records rather than a third copy.
A governed writer MUST preserve each retained source and useful correction
history; a comparison proof does not itself combine, tighten or mutate files.
The admitting/maintaining operation MUST perform its other evidence, lifecycle,
security and observed-write checks before applying any planned action.

Contradictions and uncertainty MUST go to the detecting developer/session.
When another developer originated a conflicting claim, the session MUST identify
that attribution and SHOULD recommend clarification with them as needed. It
MUST NOT send messages automatically. Only disputed learning and dependent work
need wait. Routine successful comparison SHOULD stay quiet; failures, conflicts
and consequential corrections MUST surface promptly at the next boundary.

## Recheck before applying and again before landing

```sh
python3 -I "$SKILL_DIR/scripts/compare_learning.py" check --repo /path/to/repo \
  --request /private/path/comparison.json --proof /private/path/proof.json
```

`proof.json` MUST hold exactly the `proof` object from review, not the result
envelope. Check repeats the complete inspection and judgment validation. A changed
candidate, route, existing source, match set or proof MUST refuse. It reuses no
cached body and writes no canonical data. A governed writer MUST revalidate the
comparison against its locked observed bank immediately before mutation.

The observation covers local canonical bytes only. It does not fetch or verify
`main`, prove code behavior or complete semantic review for the agent. Before
publication the session MUST obtain the latest landed bank through the governed
sharing protocol and repeat this comparison there. A newly landed duplicate or
correction MUST trigger rereading and a new judgment; a previous empty match set
is not permission to skip that work. Offline/unavailable sharing remains pending,
while local learning and its evidence remain useful.

Exit 0 means a completed inspection or structurally valid agent judgment/proof
check. It is not admission, publication, semantic correctness or authorization.
Exit 1 means unsafe, incomplete, unavailable or changed input/state; it supplies
no proof of absence. Private content-free audits are reported separately from
canonical writes. The helper executes no remembered commands, sends no messages,
requires no daemon and changes neither bank, code, index nor HEAD.
