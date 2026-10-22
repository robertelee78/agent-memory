---
id: ADR-0005
status: proposed
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002, ADR-0003]
---

# ADR-0005: Separate admission, consolidation, retrieval, and forgetting

## Context

Many memory systems treat “add” as one opaque model call and “search” as vector
similarity. Research shows distinct failure modes at memory formation,
granularity, contradiction handling, selection, and reading. A reliable system
needs independently testable stages.

## Decision

The memory pipeline SHALL expose these distinct operations:

```text
capture → classify → admit/quarantine → consolidate → project/index
       → retrieve → assemble → use → observe outcome → correct/forget
```

The implementation SHALL define its memory state machine once in a versioned,
machine-readable schema used by the CLI, validator, tests, and generated model
guide. The initial lifecycle states are `candidate`, `quarantined`, `active`,
`superseded`, `revoked`, `expired`, and `deleted`.

Legal transitions SHALL be explicit. In particular:

- admission moves a candidate or reviewed quarantine to active;
- correction creates a new version and supersedes the incorrect version;
- supersession names the active replacement;
- revocation deactivates an unsafe or invalid memory without requiring a
  replacement;
- expiration deactivates a time-bounded memory;
- reactivation requires new evidence and a new event rather than silently
  changing the old record;
- deletion is terminal for that record ID and leaves only the safe tombstone
  defined by ADR-0010.

Confidence, trust, visibility, and valid time are independent axes, not aliases
for lifecycle. The current-selection predicate SHALL require authorized scope,
active lifecycle, applicable valid time, sufficient trust for the operation, and
no active superseding record. Documentation and runtime MUST use the same
predicate or fail a contract test.

### Admission

Admission SHALL consider future utility, evidence confidence, novelty,
durability, scope, content type, and security classification. It SHALL prefer
small, typed, task-stage-aligned records over whole-session summaries.

Direct user statements and verified tool results MAY qualify for deterministic
rules. Assistant inferences and external content SHALL require stronger
evidence or review. Procedures and global preferences MUST NOT be promoted from
untrusted content automatically.

### Consolidation

Consolidation SHALL run outside the foreground task by default. It MAY merge
duplicates, propose higher-order patterns, link related memories, or suggest
procedural promotion. It MUST retain source links, be idempotent, and emit
events rather than destructively rewriting history.

### Retrieval

Retrieval SHALL apply hard filters before ranking:

1. principal and project authorization;
2. active validity state and time;
3. trust threshold and memory type;
4. directory, component, branch, or task-stage scope when applicable.

Ranking SHOULD combine exact identifiers, full-text search, semantic similarity,
recency or reinforcement, graph distance, and negative evidence such as a
superseding or failed record. The retriever SHALL be able to explain why each
memory was selected.

The baseline retriever SHALL use the bounded `MEMORY.md` route map, exact
paths/identifiers, and ordinary text search. Embeddings and graph traversal are
optional ranking signals. Disabling or deleting them MUST leave the baseline
workflow intact.

### Assembly

Context assembly SHALL enforce a token budget, preserve source and status
labels, include contradictory evidence when unresolved, and prefer diversity
across relevant memory types over near-duplicate results.

### Forgetting

The system SHALL support expiration, supersession, revocation, user deletion,
and policy retention as separate operations. Forgetting SHALL propagate to all
materialized views and derived indices. A deleted memory MUST NOT reappear from
a cached index or consolidation output.

## Consequences

- Each memory failure can be attributed to a stage.
- Simple deterministic policies can ship before learned routers.
- Background consolidation improves quality without blocking coding work.
- More lifecycle state and test fixtures are required than for CRUD plus vector
  search.

## Alternatives considered

- **Store every transcript turn:** rejected because noise, privacy risk, and
  poisoning scale with volume.
- **One model decides every operation:** rejected because it is expensive,
  opaque, and difficult to reproduce.
- **Last-write-wins facts:** rejected because it erases provenance and cannot
  express unresolved conflict or historical validity.
