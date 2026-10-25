# projectbrief

## Documentation to inspect

- [README.md](../README.md)
- [docs/mission.md](../docs/mission.md)
- [docs/releases/v0.0.2.md](../docs/releases/v0.0.2.md)

## Reviewed intent: shared-project-memory

Agent-memory provides durable, source-linked repository context across harnesses, sessions, separate worktrees and machines; ordinary Markdown is the authority for memory.

Applicability: `{"conditions": {}, "scope": "repository"}`.
Reviewed value: `"One repository-bound shared Markdown bank"`.
Evidence: [docs/adr/0001-local-first-authority-boundaries.md](../docs/adr/0001-local-first-authority-boundaries.md), [docs/adr/0010-memory-bank-file-contract.md](../docs/adr/0010-memory-bank-file-contract.md), [docs/adr/0012-git-shared-memory.md](../docs/adr/0012-git-shared-memory.md).

## Reviewed intent: authority-boundaries

Code/tests establish behavior, approved ADRs establish intent, AWA establishes work/delivery, and Vox establishes live coordination. Memory supplies context and links, not a duplicate tracker or permission source.

Applicability: `{"conditions": {}, "scope": "repository"}`.
Reviewed value: `"Authority remains with the relevant current source"`.
Evidence: [docs/adr/0001-local-first-authority-boundaries.md](../docs/adr/0001-local-first-authority-boundaries.md), [docs/adr/0012-git-shared-memory.md](../docs/adr/0012-git-shared-memory.md).
