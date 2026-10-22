---
id: ADR-0005
status: accepted
approval-scope: v0.0.1
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002, ADR-0003]
---

# ADR-0005: Separate admission, consolidation, retrieval, and forgetting

Accepted for **v0.0.1 only**, as recorded in the
[operator approval](../design/v0.0.1-approval.md). Later-milestone proposals
remain under review. Implementation, acceptance and delivery require separate
evidence.

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

The skill SHALL automatically capture durable discoveries, verified fixes,
useful failed approaches, user corrections, and confirmed project decisions
when they would materially change a future session's behavior. It MUST NOT ask
for approval for each routine memory. Assistant proposals and unanswered
recommendations MUST NOT become confirmed decisions; uncertainty and unverified
inferences SHALL remain labeled. Raw transcripts, easily recovered source facts,
and duplicate tracker state MUST NOT accumulate as learning records.

Before creating or updating a memory, the skill SHALL consult the bounded
router, search existing memory for the same concept and alternate wording, and
read plausible matches with their evidence and conditions. The same learning
SHALL update or combine with its existing record; distinct knowledge SHALL keep
a separate identity. Unavailable or interrupted memory MUST NOT be treated as
proof that no match exists. The first release SHALL support this semantic
comparison using ordinary search and agent judgment without requiring vectors.
Before publication the skill SHALL repeat the comparison against the latest
landed memory, so concurrent sessions cannot bypass deduplication.

### Consolidation

Consolidation SHALL run at natural turn boundaries outside foreground work by
default. It SHALL automatically combine duplicate memories and tighten wording
while preserving distinct conditions, evidence, developer/session provenance,
and useful correction history. It MAY propose higher-order patterns, link
related memories, or suggest procedural promotion. It MUST retain source links,
be idempotent, and emit events rather than destructively rewriting history.
Unresolved changes in meaning SHALL route to the detecting developer as defined
by ADR-0012. Routine maintenance SHALL be quiet; conflicts, failures, and
consequential corrections SHALL be surfaced promptly at a boundary.

### Retrieval

Retrieval SHALL apply hard filters before ranking:

1. project authorization and applicability;
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

**Growing-bank recall in v0.0.2.**

`v0.0.2` SHALL improve recall quality as the shared project bank grows. Evaluation
SHALL use representative task queries and increasing bank sizes, including
paraphrases, conditional lessons, corrections, duplicate candidates, and stale
records. It SHALL measure relevant-memory retrieval, inappropriate or missed
recall, context size, and retrieval cost against the v0.0.1 baseline. Workloads
and acceptance targets SHALL be agreed before claiming improvement; exact
thresholds remain an open product decision rather than invented requirements.

The initial scale envelope SHALL use banks of 1,000, 10,000, and 50,000 distinct
durable lessons. These are test workloads, not accumulation goals or counts of
files. The 50,000-lesson workload SHALL be used to investigate retrieval quality
at the upper bound; its session-contention combination remains exploratory
under ADR-0009. Fixture size SHALL also record bytes/tokens and record lengths.

A fixed, labeled query suite SHALL run across increasing sizes with realistic
additional distractors, paraphrases, duplicate candidates, superseded lessons,
and branch-specific conditions. Ground truth SHALL identify relevant records
and applicable versions/conditions. Instrumentation SHALL measure relevant
lessons found and missed; irrelevant, stale, or incorrectly applicable guidance;
latency; retrieved-context size; and retrieval cost. Measurements SHALL compare
quality and cost curves across sizes and with the v0.0.1 baseline, exposing
slopes or sudden degradation rather than merely proving storage capacity.

Preserving retrieval quality as the bank grows SHALL be an acceptance objective,
not an assumed result. The agreed query suite, measured baseline, tolerances,
and failure criteria SHALL be recorded before a performance pass is claimed.
Exact tolerances remain to be derived from the data and operator review; no
unmeasured scalability or absence-of-degradation claim is permitted.

Scope, trust, validity, provenance, and applicability filters MUST remain intact.
A larger bank MUST NOT imply loading everything into every turn or introducing
a required vector store. Lessons from actual v0.0.1 use MAY add further scope.

### Assembly

Context assembly SHALL enforce a token budget, preserve source and status
labels, include contradictory evidence when unresolved, and prefer diversity
across relevant memory types over near-duplicate results.

### Forgetting

The system SHALL support expiration, supersession, revocation, user deletion,
and policy retention as separate operations. Forgetting SHALL propagate to all
materialized views and derived indices. A deleted memory MUST NOT reappear from
a cached index or consolidation output.

The skill SHALL automatically prune incorrect or no-longer-relevant guidance
when stronger evidence establishes the correction or loss of applicability.
Age alone MUST NOT justify pruning. A memory that is obsolete on `main` but
still applies to an older branch or environment SHALL retain that narrower
condition. Pruning SHALL remove the claim from active guidance while preserving
a concise safe correction or supersession explanation; explicit deletion SHALL
follow the content-free tombstone contract. Ambiguous evidence SHALL prompt
clarification rather than silent removal. The baseline capture, deduplication,
consolidation, scoped retrieval, and pruning loop SHALL ship in `v0.0.1`.

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
