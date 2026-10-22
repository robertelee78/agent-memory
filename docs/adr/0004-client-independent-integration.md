---
id: ADR-0004
status: proposed
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002]
---

# ADR-0004: Client-independent core with CLI, MCP, and adapters

## Context

Codex, Claude Code, and OpenCode have different instruction files, hooks,
plugin formats, and subprocess lifecycles. MCP provides a common tool surface
but does not guarantee automatic capture or recall. Hook-only designs become
client products rather than portable memory systems.

## Decision

The portable core SHALL be the Memory Bank file contract plus the skill that
operates it. A deterministic local CLI SHALL validate and maintain that
contract. MCP SHALL be a thin protocol adapter over the same files and CLI.
Client plugins, skills, and lifecycle hooks SHALL be optional adapters.

The first release SHALL provide the shared skill and deterministic helpers for
automatic durable capture, search-before-write, consolidation, pruning, and the
memory-only Git exchange in ADR-0012. Claude Code, Codex, OpenCode, and custom
harnesses SHALL use the same bank and protocol. Harness-specific hooks and MCP
integration MAY follow; their absence MUST NOT prevent this workflow.

Failure or absence of the CLI beyond installation MUST NOT prevent a client
from reading the current bank or making the constrained single-file manual
checkpoint described by ADR-0010. That fallback does not provide mediated
admission, authorization, concurrency, audit, or verified-receipt guarantees.
Full governed mutations require the CLI or the same packaged transaction helper.

The CLI MAY grow to cover these capability groups:

```text
agent-memory status|doctor
agent-memory context|search|get|explain
agent-memory capture|remember|checkpoint
agent-memory correct|supersede|revoke|forget
agent-memory project|compact|rebuild|migrate
agent-memory eval
```

The MCP server SHALL expose typed equivalents, use the caller's resolved
project and principal identity, and refuse ambiguous project selection. Tool
responses SHALL include memory IDs, scope, validity, provenance, and whether a
result is current or historical.

Adapters SHALL follow these rules:

- A client hook MAY capture candidate observations but MUST NOT bypass durable
  admission policy.
- A first-turn hook MAY request bounded context but MUST NOT inject global or
  cross-project memory implicitly.
- A client instruction file SHALL be a short activation shim, not a fork of the
  core policy.
- A client-specific capability failure SHALL degrade to explicit CLI or MCP
  commands, not change memory semantics.
- Installing or updating one adapter MUST NOT overwrite unrelated user config.
- All adapters SHALL identify the actual client, session, repository, and
  operation in captured provenance.

Principal and project identity SHALL follow
[ADR-0011](0011-principal-and-project-identity.md). No adapter MAY substitute a
directory basename, active credential, or client-local project identifier.

The implementation SHOULD support Codex `AGENTS.md`, OpenCode `AGENTS.md` and
`instructions`, and Claude Code `CLAUDE.md`/`AGENTS.md` without requiring those
files to share identical syntax. Generated, sentinel-delimited activation
blocks are preferred to whole-file symlinks.

## Consequences

- The system remains usable from scripts and future clients.
- MCP supplies broad interoperability while hooks improve ergonomics.
- Adapter tests must run against real client-visible configuration and tool
  schemas, not only internal functions.
- Perfect automatic end-of-session capture cannot be assumed; checkpoints and
  pending-event recovery are required.

## Alternatives considered

- **MCP only:** rejected because tools alone do not provide reliable lifecycle
  capture or progressive first-turn context.
- **One plugin implementation per client:** rejected because behavior and state
  would drift.
- **Instruction files only:** rejected because free-form direct edits cannot
  enforce schemas, locking, provenance, or deletion.
