# Project brief

agent-memory gives a fresh coding agent entering a project a quick picture of
what the project is and why, how it is built, and its lessons (fixes, dead ends,
the user's corrections and decisions). Memory is plain Markdown in the
repository's `memory-bank/`, shared through Git. It never makes the user wait.

## Authority

- **Product requirements:** [docs/mission.md](../docs/mission.md) plus ADR-0001–0012,
  and the operator's lean design [docs/design/v0.0.15-lean.md](../docs/design/v0.0.15-lean.md)
  (2026-11-15), which wins where they differ. Requirements added later by agents
  (seals, proofs, frontier/holds, layout recovery, re-verification, unit/contract
  test suites) were withdrawn; see the notice in [docs/adr/README.md](../docs/adr/README.md).
- **Behaviour:** the code in `src/` and real use measured by `scripts/qa.sh`.
- **Work and delivery:** AWA issues and milestones; live coordination: Vox.
  Memory gives context and links, not permission.

## Boundaries

One repository-bound shared bank. No personal or per-developer memory, no
database or daemon, no MCP server, no cross-project federation (deferred).
