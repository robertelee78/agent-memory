# Architecture decision records

These ADRs were derived from the
[state-of-the-art review](../research/2026-10-21-agent-memory-state-of-the-art.md).
The operator accepted their v0.0.1 scope in the
[recorded design approval](../design/v0.0.1-approval.md).

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
| [0001](0001-local-first-authority-boundaries.md) | Local-first memory and authority boundaries | Accepted (v0.0.1) |
| [0002](0002-markdown-first-temporal-memory.md) | Markdown-first, typed, temporal memory model | Accepted (v0.0.1) |
| [0003](0003-progressive-disclosure.md) | Progressive disclosure for memory and skills | Accepted (v0.0.1) |
| [0004](0004-client-independent-integration.md) | Harness-independent skill and CLI | Accepted (v0.0.1) |
| [0005](0005-admission-consolidation-retrieval.md) | Separate admission, consolidation, retrieval, and forgetting | Accepted (v0.0.1) |
| [0006](0006-security-and-evaluation.md) | Security boundaries and executable evaluation | Accepted (v0.0.1) |
| [0007](0007-portable-pack-and-lifecycle.md) | Portable skill pack and installation lifecycle | Accepted (v0.0.1) |
| [0008](0008-project-bank-and-optional-federation.md) | Project Memory Bank first; optional ecosystem federation | Accepted (v0.0.1) |
| [0009](0009-contract-tests-and-fault-injection.md) | End-to-end contract tests and incident-derived regressions | Accepted (v0.0.1) |
| [0010](0010-memory-bank-file-contract.md) | Concrete Memory Bank file and scope contract | Accepted (v0.0.1) |
| [0011](0011-principal-and-project-identity.md) | Stable developer attribution and project identity | Accepted (v0.0.1) |
| [0012](0012-git-shared-memory.md) | Git-shared memory across worktrees, machines, and developers | Accepted (v0.0.1) |
| [0013](0013-public-cli-and-corrective-releases.md) | AWA-style public CLI and corrective release tracking | Accepted (v0.0.2) |
| [0014](0014-useful-shared-memory-quality.md) | Useful shared memory and lifecycle assurance | Proposed (v0.0.3) |

[ADR-0013](0013-public-cli-and-corrective-releases.md) defines the corrective
`v0.0.2` public command contract and the current future-release mapping. Older
future-version labels in earlier ADRs remain historical; their underlying
reliability, hook and federation scopes are preserved by that mapping.

[ADR-0014](0014-useful-shared-memory-quality.md) proposes v0.0.3 quality corrections
from the real-bank mission review. Its proposed status grants no design approval.
The operator's later scope decision, recorded there, brings existing signing task
#49 into v0.0.3 and defines consumer-memory isolation while allowing source clones
to contain this repository's development bank.

## Status values

- **Proposed:** under review; implementation must not treat it as settled.
- **Accepted:** approved as the current architectural direction.
- **Superseded:** replaced by another ADR, which must be linked.
- **Rejected:** considered and deliberately not adopted.

An Accepted ADR with `approval-scope: v0.0.1` approves only the exact first-release
work keys in the approval record. Later-release proposals in that document
remain under review and receive no design approval from this status. Withdrawn
capabilities remain withdrawn. Acceptance is a design decision; implementation,
independent acceptance and delivery require their own evidence.

Changes to an accepted decision should add a new ADR rather than silently
rewriting the old decision. Clarifications that do not alter the decision may
edit the existing record while preserving its history in Git.

## Work accountability

Each ADR has one repository-linked GitHub Project containing its root epic and
native sub-issues. [Release milestones](https://github.com/robertelee78/agent-memory/milestones)
group tasks across those Projects, beginning with `v0.0.1`. The ADRs remain the
authority for requirements; issue excerpts are generated planning records.
Proposed ADRs do not authorize implementation or make their tasks execution-ready.

Every ADR mutation MUST reconcile its managed work through the
`github-work-accountability` (`awa`) workflow. Before editing, inspect the
committed ADR, this policy, and its existing work keys. After committing, regenerate
and validate the source-bound extraction against the immutable commit and, when
the source is expected there, the default branch. Update the existing issues by
stable work key and reconcile the document's Project. Read back the new source
commit and blob and verify that Source freshness is Current before claiming
synchronization. If GitHub cannot be updated, retain the pending reconciliation
and report it explicitly. Renames, supersession, and deletion preserve work
history and require explicit treatment of unfinished obligations.

Changes to a referenced design contract MUST also reconcile the tasks that
depend on it.
