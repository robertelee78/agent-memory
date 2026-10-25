# productContext

## Reviewed intent: supported-workflow

The target workflow includes Claude, Codex, OpenCode and custom harnesses, with each concurrent coding session in its own branch/worktree and Git carrying landed project memory across computers.

Applicability: `{"conditions": {}, "scope": "repository"}`.
Reviewed value: `"Harness-agnostic, same-repository Git continuity"`.
Evidence: [docs/adr/0012-git-shared-memory.md](../docs/adr/0012-git-shared-memory.md), [docs/mission.md](../docs/mission.md).

## Reviewed intent: scope-exclusions

Personal profiles and per-developer memory overlays are excluded. Cross-repository sharing is deferred; MCP is excluded. This bank contains only agent-memory project context.

Applicability: `{"conditions": {}, "scope": "repository"}`.
Reviewed value: `"Shared project memory only"`.
Evidence: [docs/adr/0001-local-first-authority-boundaries.md](../docs/adr/0001-local-first-authority-boundaries.md), [docs/adr/0013-public-cli-and-corrective-releases.md](../docs/adr/0013-public-cli-and-corrective-releases.md).
