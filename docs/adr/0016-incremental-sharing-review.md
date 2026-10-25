---
id: ADR-0016
status: proposed
date: 2026-10-25
amends: [ADR-0012]
related: [ADR-0005, ADR-0009, ADR-0014]
---

# ADR-0016: Changed-learning sharing review

Proposed technical refinement for existing task #99; not implementation approval.
It preserves ADR-0012's product obligations and does not add a duplicate outcome.

## Context

The v1 helper's whole-bank all-pairs requirement and response limits refuse valid
small banks. ADR-0012 requires plausible-match review, cross-file conflict handling
and source integrity, and explicitly does not promise perfect contradiction
recognition. Rechecking every unchanged pair is an implementation restriction,
not the operator's requested workflow.

## Decision

A v2 sharing proof SHALL pin complete actual base/local/landed inventories and
semantic framing, while declaring its actual changed-learning and plausible-match
review boundary. The skill MUST search/read existing plausible matches before
novelty, consolidation or publication judgments. It MUST include actual concurrent
versions, explicit lifecycle links and affected changed framing, and MUST examine
meaning across files rather than infer compatibility from a clean text merge.

Selected evidence SHALL be paged without silent truncation. A sealed proof MUST
cover the complete selected interactions and remain bound to current actual
sources/scope. Unknown, unsafe, hidden or uncertain plausible evidence MUST NOT
become an absence claim. Unreviewed old pairs MUST NOT be labeled compatible or
represented as a global consistency certificate. The agent's search and judgments
remain semantic responsibilities; a lexical miss is not a theorem.

Existing dispute/dependency holds, provenance/conditions, attribution, invalidation
and terminal deletion protections SHALL remain. The detecting developer SHALL
receive conflicts; unrelated work SHALL continue. No winner by recency/identity
or automatic message to another developer is permitted.

Whole-data observation and private artifact bounds SHALL be distinct from bounded
agent disclosure. Batch Git reads MUST check complete inventories and exact blob
identities/sizes without silently skipping data. The existing observed transaction,
memory-only publication/refresh and recovery boundaries SHALL remain authoritative.
No required database, personal memory, cross-repository transfer, MCP or daemon
is introduced; Markdown format remains unchanged and local review state disposable.

The [technical design](../design/v0.0.3-daily-use-repair.md) and agreed #99 workload
qualification define the implementation/acceptance boundary. Old proof formats
retain explicit handling; unknown formats MUST refuse rather than be reinterpreted.

## Consequences

- Review work follows the new learning and its plausible existing relationships.
- Source inventory remains complete; semantic claims explicitly state their scope.
- Human-like semantic review remains fallible and cannot be sold as perfect proof.
- Large-bank usability requires repairing the full pipeline, not raising one cap.
