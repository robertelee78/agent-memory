---
id: ADR-0008
status: proposed
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002]
---

# ADR-0008: Project Memory Bank first; optional ecosystem federation

## Context

Cline's Memory Bank is unusually effective for one-off projects because the
entire system is a small directory of understandable Markdown files. A trusted
practitioner's experience independently suggests a different scale boundary:
simple Memory Banks for individual projects, and knowledge graphs only for the
few ecosystems whose cross-project coupling is actively maintained.

Making every project participate in a global graph would add identity,
synchronization, schema, retrieval, and operational failure modes to the common
case. Conversely, treating every repository as isolated loses useful knowledge
when several repositories really are parts of one shared product or ecosystem.

## Decision

The default unit of memory SHALL be one project-local `memory-bank/`. It SHALL
remain independently readable, editable, versionable, exportable, and useful.
No federation service or global catalog is required to initialize, checkpoint,
resume, search, validate, or repair it.

Cross-project memory SHALL be an explicit opt-in federation. A federation SHALL
name its member projects and stable project identities in a human-readable
manifest. Membership MUST NOT be inferred only from directory names or recent
access.

The first federation capability SHOULD be a catalog of project identities,
descriptions, repository locations, and explicit typed links. A knowledge graph
MAY be added when actual queries require multi-hop relationships, temporal
edges, or shared entities. The graph SHALL be a rebuildable projection over
project exports and federation-owned records, never the sole authority for a
project's bank.

A project SHALL publish only deliberately exported memories to a federation.
Unadmitted session/branch observations, quarantined, secret-bearing, and deleted records MUST
NOT become cross-project merely because they are locally retrievable. A
federation query MUST preserve the source project, evidence, validity, trust,
and access scope of every result.

The system SHALL degrade cleanly:

- removing an index or graph does not alter a project bank;
- an unavailable federation does not block local memory;
- removing a project from a federation removes its projected content without
  deleting the project's own bank;
- a relation may point to a missing project without corrupting either bank.

## Consequences

- The one-off project path stays as simple and robust as Cline's methodology.
- Complexity is paid only by users with a real project ecosystem.
- Cross-project sharing requires explicit identity and export governance.
- The graph can evolve or be replaced without migrating primary project memory.

## Alternatives considered

- **One global knowledge graph for all projects:** rejected because it burdens
  the common case and expands leakage and operational risk.
- **Never support cross-project memory:** rejected because coupled ecosystems
  have legitimate relational and reusable-procedure needs.
- **Automatically ingest every project memory into a graph:** rejected because
  local visibility is not consent for federation.
