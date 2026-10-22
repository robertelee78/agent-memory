---
id: ADR-0001
status: proposed
date: 2026-10-21
---

# ADR-0001: Local-first memory and authority boundaries

## Context

The system must share useful context across Codex, Claude Code, OpenCode, and
future clients without making any client, cloud service, transcript, or model
the durable authority. Coding repositories already contain stronger sources for
many facts: source code, tests, Git history, planning documents, issue trackers,
release systems, and explicit user decisions.

A memory layer that duplicates those authorities without provenance will drift.
A global pool that mixes projects or people will leak information. A client-
specific store cannot provide cross-client continuity.

## Decision

`agent-memory` SHALL be a local-first, client-independent layer for durable
context that cannot be recovered cheaply or reliably from the current
authoritative source. Its minimum viable form SHALL be a project-local set of
ordinary Markdown files plus a portable skill. Reading, editing, checkpointing,
and resuming a bank MUST NOT require a database, service, daemon, embedding
model, knowledge graph, MCP server, or client hook.

The core SHALL NOT require a hosted service, a particular model, a particular
agent client, lifecycle hooks, or an orchestrator. Optional synchronization MAY
be added without changing the local data model.

The system SHALL distinguish these scopes:

- principal-wide preferences and feedback;
- repository-wide project knowledge;
- directory or component context;
- worktree, branch, task, and session observations;
- explicitly shared organizational memory.

Repository scope SHALL be the default for coding memories. Cross-repository and
principal-wide retrieval MUST be explicit.

Memory SHALL NOT replace these authorities:

| Fact | Authority |
| --- | --- |
| Current implementation behavior | Code and executable tests |
| Product intent and acceptance | Current approved planning source |
| Work progress and delivery | Issue tracker, CI, integration, and release evidence |
| Agent ownership or coordination | The active coordination system |
| User preference | Direct user statement or user-approved profile change |
| Historical attempt or observation | Immutable memory event and its evidence |
| Reusable procedure | Reviewed, versioned skill or rule |

When memory conflicts with a stronger current authority, the agent MUST follow
the stronger authority and SHOULD record a candidate revocation or correction.
Retrieved memory is context, not enforced policy and not proof.

## Consequences

- The first implementation can work offline and remain inspectable.
- GitHub/AWA and Vox can coexist without duplicated lifecycle or ownership
  state; memory stores links and durable lessons, not a second tracker.
- Repository identity and scope resolution become security-critical.
- Cross-machine synchronization is deferred and must preserve provenance,
  tombstones, and conflicts rather than using blind last-write-wins.

## Alternatives considered

- **Hosted memory service as authority:** rejected for the core because it adds
  availability, privacy, lock-in, and audit dependencies.
- **One global memory namespace:** rejected because relevance and access-control
  failures become unavoidable.
- **Commit every memory into the product repository:** rejected as the only
  mode because personal preferences and noisy observations do not belong in
  shared product history. Curated export MAY be supported.
