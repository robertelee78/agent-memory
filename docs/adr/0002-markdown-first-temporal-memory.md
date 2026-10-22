---
id: ADR-0002
status: proposed
date: 2026-10-21
depends-on: [ADR-0001]
---

# ADR-0002: Markdown-first, typed, temporal memory model

## Context

Flat Markdown banks are inspectable and resilient, but unstructured prose alone
does not clearly express provenance, corrections, deletion, or changing truth.
Mutable profiles are compact but can silently lose information. Append-only
transcript stores retain too much noise and allow stale facts to remain active.

Research on temporal graphs, revocable memory, and coding-agent event logs
supports retaining evidence and explicit validity. Cline's Memory Bank shows
that the ordinary files must remain usable as the product, rather than becoming
mere renderings of a hidden database.

## Decision

The canonical project memory SHALL be human-readable Markdown under a
`memory-bank/` directory. When the bank is tracked, Git history and linked source
evidence provide the baseline audit trail. An intentionally untracked/private
bank has reduced history unless the user configures a separate protected backup
or versioned store; the product SHALL report that condition rather than claiming
Git auditability. The current bank MUST remain complete and usable without any
generated index or local application state.

The initial bank SHALL preserve Cline's recognizable project model:

- `projectbrief.md` — stable goals and boundaries;
- `productContext.md` — user problem and intended experience;
- `activeContext.md` — current focus, recent changes, open questions, next step;
- `systemPatterns.md` — architecture and established patterns;
- `techContext.md` — stack, setup, commands, and constraints;
- `progress.md` — validated capabilities, remaining work, and known problems;
- `MEMORY.md` — a bounded router describing those files and any added topics.

Projects MAY omit irrelevant documents or add topic files. Initialization SHALL
start with the smallest useful set and evolve it rather than producing empty
boilerplate.

The exact route, optional-directory, private-overlay, direct-edit, transaction,
and tombstone contract is defined by [ADR-0010](0010-memory-bank-file-contract.md).

Durable changes that need correction, revocation, or historical explanation
SHALL use explicit status and source links in the relevant file or a linked
decision/episode record. Git history MUST NOT be the only indication that a
currently visible statement is stale.

An optional append-only historical event journal MAY record these mutation kinds:

- `observation-captured`;
- `memory-admitted`;
- `memory-corrected`;
- `memory-superseded`;
- `memory-revoked`;
- `memory-expired`;
- `memory-deleted`;
- `memory-promoted-to-procedure`;
- `projection-built`.

The historical journal supports deeper audit, synchronization, and rebuilding
optional projections. It is distinct from the required short-lived recovery
marker/journal for a mediated multi-file mutation. Neither MAY contain the only
readable copy of current project memory. Losing historical events may lose
auxiliary history but MUST NOT make a clean current bank unusable.

Initial memory kinds SHALL include semantic fact, user feedback, project
decision, episodic attempt, result, reference, and procedural candidate.

Every standalone admitted record SHALL carry:

- a stable memory ID and schema version;
- a resolved scope, which MAY inherit the containing project or private overlay;
- memory kind and lifecycle state;
- source actor, source kind, and evidence reference;
- recorded time and applicable valid time when known;
- links to records it derives from, corrects, or supersedes.

Confidence, admission reason, trust, and redaction classification SHALL be
recorded when the admission policy or content class requires them. Content
hashes MAY be computed for observed-write and projection checks but SHOULD NOT
be stored in a tombstone.

Revocation and supersession SHALL remove a memory from current guidance while
preserving the minimum safe explanation or evidence required for audit.
Deletion SHALL create a durable tombstone when another artifact could recreate
the content; projections and indices MUST remove deleted content and MUST NOT
resurrect it during rebuild or consolidation. Tombstones MUST NOT retain deleted
content or reversible or low-entropy content hashes.

Mediated per-file writes SHALL use observed-content checks and atomic
replacement. Conflicting concurrent edits MUST stop for reconciliation rather
than silently using last write wins. Every mediated mutation SHALL hold the
exclusive per-bank lock defined by ADR-0010 from inventory through readback.
Mediated multi-file writes SHALL also use its detectable recovery protocol and
MUST NOT claim global atomicity. An optional historical-journal writer SHALL use
idempotency keys. A generated projection MUST record the source file revisions
or digest from which it was built.

A local SQLite database MAY provide transactional metadata and full-text search,
but it SHALL be disposable. A database-only record MUST NOT be the sole durable
representation of admitted project memory.

## Consequences

- Mediated standalone records can explain why a memory was visible; direct prose
  edits retain ordinary Markdown/Git history rather than a complete access log.
- The ordinary bank remains understandable and recoverable without tooling.
- Synchronization and migrations can operate on explicit events.
- Records that need lifecycle semantics are more structured than Cline's base
  files, while ordinary project context can stay as concise prose.

## Alternatives considered

- **An event log as the only canonical state:** rejected because it makes the
  simplest useful workflow depend on projection machinery.
- **SQLite as the only canonical state:** rejected because portability,
  inspectability, and recovery would depend on one binary representation.
- **A temporal graph as the initial canonical store:** rejected as premature
  operational complexity. A graph projection remains possible.
