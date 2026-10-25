---
kind: operation
domain: recall
---

# Recall current project memory

## Use when

A session SHOULD use this operation for current file, record-ID or ordinary-text recall in the selected repository. For a bounded harness payload, it MUST subsequently select context assembly; a result-count limit is not a token budget.

## Required context

A session using this leaf MUST first load and follow the [recall domain](../recall/GUIDE.md). It MUST open the [shared model](../model/GUIDE.md) only when an unresolved semantic question requires it.

The recall domain owns the complete shared request, source/selection gates and bounded-observation contract. The session MUST apply those rules, not a separate client policy.

## Inputs and preconditions

A session MUST prepare the exact shared nine-field recall request from this domain using the actual selected project and truthful current selectors/context. It MUST keep the request private and preserve unknown conditions as unknown.

## Observe

A session MUST resolve current repository/developer context and inspect the actual bank/interruption state before requesting current guidance. It MUST obtain non-Git condition values from real observations; the helper observes the branch and revision.

## Plan

The session MUST select relevant IDs/files/query and explicit trust/kinds/limits without broadening applicability. This operation plans no canonical changes; private content-free audit writes remain distinct from memory/code changes.

## Apply

With `SKILL_DIR` resolved to this package:

```sh
python3 -I "$SKILL_DIR/scripts/retrieve.py" --repo /path/to/repo \
  --request /private/path/recall.json
```

## Read back

A session MUST read the actual selected items, their matching explanations, metadata/conditions/source hashes and exclusion counts. It MUST preserve full source labels and distinguish orientation-only prose from admitted typed records.

Managed active-context checkpoints MUST match their explicit conditions against
actual branch/HEAD and truthful supplied task/component context before ranking.
Unknown, missing, malformed or mismatched conditions exclude the checkpoint,
including exact current lookup. Matching context retains `assignment: false`
and evidence availability qualifications; matching bytes do not prove a claim
true or remotely published. Ordinary manual prose remains unassessed orientation;
this helper does not infer arbitrary statement-level conditions. A session MAY
use explicit [historical inspection](../bank/inspect/GUIDE.md) to inspect an
unrelated checkpoint without weakening current recall gates.

## Verify

A session MUST verify its complete current observation under the shared domain rules before relying on a result. It MUST re-evaluate changed evidence and time/applicability for consequential use, and revalidate under the write lock before a governed mutation.

## Outcomes

A session MUST interpret the actual base recall result using the shared domain outcome contract. A completed evaluation may legitimately select no current records; it is distinct from a failed or changed observation.

A session preparing bounded harness context MUST continue to
[assembly](../assembly/GUIDE.md) after recall. Recall's result limit is not a
verified harness token budget and does not bundle known contradictions by itself.

## Failure handling

If the bank, scope, safety, audit or current source checks fail, the session MUST retain local work and resolve the actual cause before retrying. It MUST NOT substitute a successful empty selection for that failure or delete unknown locks to force a read.

## Diagnostic fallback

If only file tools are available, a session MAY inspect the same bounded router and relevant files through the bank’s file-only workflow. It MUST independently apply the same scope, source and lifecycle qualifications and MUST NOT claim mediated recall, audit or token-budget verification.
