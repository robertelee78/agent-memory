# agent-memory

A portable, local-first Memory Bank for coding agents.

## Mission

Take what is good about
[Cline's Memory Bank](https://docs.cline.bot/best-practices/memory-bank)—small,
ordinary Markdown files; an obvious project model; explicit checkpoint and
resume; human inspectability; and no service dependency—package it with the
progressive skill and lifecycle discipline of `agent-work-accountability`, and
improve it without losing that simplicity.

The common path MUST remain useful with files, Git, and the installed skill
alone. A database, embedding model, vector index, knowledge graph, MCP server,
background daemon, or client hook may improve particular workflows, but none
may become a prerequisite for remembering and resuming a project.

The product is not “save more chat and vector-search it.” It is a maintained
project memory that helps a fresh agent understand what the project is, what
matters now, what has changed, what remains uncertain, and where stronger
evidence lives.

The design is being developed from primary research, current production systems,
and the progressive-disclosure architecture used by
`agent-work-accountability`.

## Design documents

- [Mission and product doctrine](docs/mission.md)
- [State of the art: persistent memory for coding agents](docs/research/2026-10-21-agent-memory-state-of-the-art.md)
- [Deep analysis of agent-work-accountability](docs/research/2026-10-21-agent-work-accountability-analysis.md)
- [Progressive skill blueprint](docs/design/progressive-skill-blueprint.md)
- [Install, update, and uninstall blueprint](docs/design/lifecycle-blueprint.md)
- [Architecture decision records](docs/adr/README.md)

The ADRs are currently **Proposed**. They describe the intended boundaries and
architecture; they are not claims that the implementation already exists.

Implementation planning is tracked in the repository's
[GitHub Projects](https://github.com/robertelee78/agent-memory/projects) and
[release milestones](https://github.com/robertelee78/agent-memory/milestones),
starting with `v0.0.1`. Each ADR has its own Project; release milestones span
Projects. The initial tasks are in Backlog; execution requires design approval.

The refined `v0.0.1` scope includes automatic durable capture, deduplication,
consolidation, relevant recall, and evidence-based pruning, plus
[memory-only Git publication and turn-boundary refresh](docs/adr/0012-git-shared-memory.md)
across worktrees, computers, and developers. Code integration keeps its normal
workflow. Later release milestones remain planning proposals for refinement.
