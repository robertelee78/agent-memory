---
id: ADR-0004
status: accepted
approval-scope: v0.0.1 and v0.0.4 turn-boundary hooks
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002]
---

# ADR-0004: Harness-independent skill and CLI

The v0.0.1 scope was accepted in the [operator approval](../design/v0.0.1-approval.md).
The operator subsequently approved v0.0.4 execution of turn-boundary hooks for
Claude Code, Codex, and OpenCode. Other later-milestone proposals remain under
review. Implementation, acceptance and delivery require separate evidence.

## Context

Claude Code, Codex, OpenCode, and custom harnesses need the same memory behavior.
The operator selected the AWA-style portable skill plus CLI interface and later
approved turn-boundary hooks; MCP integration remains excluded.

## Decision

The portable core SHALL be the Memory Bank file contract plus the skill that
operates it. A deterministic local CLI SHALL validate and maintain that
contract. All supported harnesses SHALL use the same files, semantic model,
commands, and verified mutation protocol.

The first release SHALL provide the shared skill and deterministic helpers for
automatic durable capture, search-before-write, consolidation, pruning, and the
memory-only Git exchange in ADR-0012. Turn boundaries SHALL invoke the common
skill/CLI operation. No hook, client plugin, or daemon SHALL be required for
the common workflow; MCP is excluded below.

MCP integration SHALL NOT be implemented in the planned releases. Its previous
proposal is withdrawn by the operator, who selected skill plus CLI as sufficient.

In `v0.0.4`, harness hooks SHALL invoke the common capture/refresh operation
automatically at turn boundaries, including when an agent forgets to invoke the
skill. Hooks SHALL supply invocation only: they MUST NOT own memory semantics,
bypass admission or Git rules, or interrupt ongoing work. Supported harness
events and integration details SHALL be established and tested through each
selected harness's public invocation surface before claiming support.
Explicit skill/CLI invocation MUST remain available to custom harnesses and
clients without hooks. Skill discovery/activation bindings are distinct from
these optional runtime callbacks.

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
- Boundary semantics and recovery belong to the skill/CLI; optional hooks may
  supply invocation without creating a second memory implementation.

## Alternatives considered

- **MCP:** excluded by the operator; no separate protocol surface is needed.
- **Hook-owned memory logic:** rejected because semantics would diverge.
  v0.0.3 callbacks invoke the common skill/CLI rather than duplicate it.
- **One implementation per client:** rejected because behavior and state drift.
- **Instruction files only:** rejected because free-form direct edits cannot
  enforce schemas, locking, provenance, or deletion.
