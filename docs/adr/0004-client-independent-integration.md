---
id: ADR-0004
status: proposed
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002]
---

# ADR-0004: Harness-independent skill and CLI

## Context

Claude Code, Codex, OpenCode, and custom harnesses need the same memory behavior.
The operator selected the AWA-style portable skill plus CLI interface. MCP and
harness-hook integrations were proposals, not requested product capabilities.

## Decision

The portable core SHALL be the Memory Bank file contract plus the skill that
operates it. A deterministic local CLI SHALL validate and maintain that
contract. All supported harnesses SHALL use the same files, semantic model,
commands, and verified mutation protocol.

The first release SHALL provide the shared skill and deterministic helpers for
automatic durable capture, search-before-write, consolidation, pruning, and the
memory-only Git exchange in ADR-0012. Turn boundaries SHALL invoke the common
skill/CLI operation; no protocol server, client plugin, hook, or daemon SHALL be
required or implemented as a planned integration capability.

MCP integration SHALL NOT be implemented in the planned releases. Its previous
proposal is withdrawn by the operator, who selected skill plus CLI as sufficient.

Harness-specific capture/recall hooks SHALL NOT be planned for these releases.
Their previous integration proposal is withdrawn. Installing a portable skill
for discovery or adding a short activation shim is distinct from implementing a
new client runtime or lifecycle-hook system.

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

Public operations SHALL resolve project identity and developer attribution,
refuse ambiguous project selection, and expose memory IDs, applicability,
validity, provenance, and current/historical status where relevant. Resolution
SHALL follow [ADR-0011](0011-principal-and-project-identity.md).

Installation bindings SHALL preserve one core policy. Client instruction files
SHALL be short activation shims, not independent implementations. Installation
or update MUST preserve unrelated user configuration. Provenance SHALL identify
the actual harness, session, repository, developer, and operation.

The implementation SHOULD support Codex `AGENTS.md`, OpenCode `AGENTS.md` and
`instructions`, and Claude Code `CLAUDE.md`/`AGENTS.md` without requiring those
files to share identical syntax. Generated, sentinel-delimited activation
blocks are preferred to whole-file symlinks. No binding MAY substitute a
client-local repository identifier or credential for the common identity model.

## Consequences

- One skill and CLI serve current and future harnesses, including scripts.
- Installation bindings are tested through real client-visible files and public
  commands; memory semantics remain shared.
- Boundary invocation and recovery are explicit skill responsibilities.

## Alternatives considered

- **MCP:** excluded by the operator; no separate protocol surface is needed.
- **Harness-specific hooks or plugins:** outside planned scope; the selected
  skill/CLI workflow is sufficient.
- **One implementation per client:** rejected because behavior and state drift.
- **Instruction files only:** rejected because free-form direct edits cannot
  enforce schemas, locking, provenance, or deletion.
