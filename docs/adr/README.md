# Architecture decision records

These ADRs are proposals derived from the
[state-of-the-art review](../research/2026-10-21-agent-memory-state-of-the-art.md).

## Normative language

Uppercase requirement terms such as **MUST**, **MUST NOT**, **REQUIRED**,
**SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**,
and **OPTIONAL** are interpreted using
[RFC 2119](https://www.rfc-editor.org/info/rfc2119/). Lowercase uses are ordinary
descriptive prose and do not create requirements.

An ADR with Proposed status describes proposed normative behavior; it does not
become an implementation commitment until accepted. Pack validation MUST check
the mechanical RFC 2119 style rules that can be proven, but keyword presence
MUST NOT be treated as evidence that a guide is semantically complete.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](0001-local-first-authority-boundaries.md) | Local-first memory and authority boundaries | Proposed |
| [0002](0002-markdown-first-temporal-memory.md) | Markdown-first, typed, temporal memory model | Proposed |
| [0003](0003-progressive-disclosure.md) | Progressive disclosure for memory and skills | Proposed |
| [0004](0004-client-independent-integration.md) | Client-independent core with CLI, MCP, and adapters | Proposed |
| [0005](0005-admission-consolidation-retrieval.md) | Separate admission, consolidation, retrieval, and forgetting | Proposed |
| [0006](0006-security-and-evaluation.md) | Security boundaries and executable evaluation | Proposed |
| [0007](0007-portable-pack-and-lifecycle.md) | Portable skill pack and installation lifecycle | Proposed |
| [0008](0008-project-bank-and-optional-federation.md) | Project Memory Bank first; optional ecosystem federation | Proposed |
| [0009](0009-contract-tests-and-fault-injection.md) | End-to-end contract tests and incident-derived regressions | Proposed |
| [0010](0010-memory-bank-file-contract.md) | Concrete Memory Bank file and scope contract | Proposed |
| [0011](0011-principal-and-project-identity.md) | Stable principal and project identity | Proposed |

## Status values

- **Proposed:** under review; implementation must not treat it as settled.
- **Accepted:** approved as the current architectural direction.
- **Superseded:** replaced by another ADR, which must be linked.
- **Rejected:** considered and deliberately not adopted.

Changes to an accepted decision should add a new ADR rather than silently
rewriting the old decision. Clarifications that do not alter the decision may
edit the existing record while preserving its history in Git.
