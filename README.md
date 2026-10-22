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
alone, with a deterministic CLI for governed operations. MCP is outside planned
scope. v0.0.3 hooks will invoke the shared capture/refresh operation at turn
boundaries. Any optional search projection must
remain disposable; it cannot become the authority for project memory.

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
- [v0.0.1 execution contract and review handoff](docs/design/v0.0.1-execution.md)

The ADRs are **Accepted for v0.0.1**, with the operator's
[design approval](docs/design/v0.0.1-approval.md) bound to the 52 first-release
tasks. Later-release proposals remain under review. Design approval does not
claim that the implementation already exists.

Implementation planning is tracked in the repository's
[GitHub Projects](https://github.com/robertelee78/agent-memory/projects) and
[release milestones](https://github.com/robertelee78/agent-memory/milestones),
starting with `v0.0.1`. Each ADR has its own Project; release milestones span
Projects. The v0.0.1 tasks have recorded design approval; GitHub tracks their
execution, with predecessor dependencies governing when each can start. Their
[ordered queue](docs/design/v0.0.1-work.json)
preserves the existing task identities. Prerequisite delivery is reviewed merge
to main, with the complete release tracked by its final publication gate.

The refined `v0.0.1` scope includes automatic durable capture, deduplication,
consolidation, relevant recall, and evidence-based pruning, plus
[memory-only Git publication and turn-boundary refresh](docs/adr/0012-git-shared-memory.md)
across worktrees, computers, and developers. Code integration keeps its normal
workflow. Later release milestones remain planning proposals for refinement.

## Development checks

Run `scripts/test.sh` from a checkout with Python 3.11+ and Git. It runs the
implemented offline public-interface suites in disposable test worlds and
reports remaining contracts as pending. `scripts/test.sh --release` rejects
incomplete release coverage. See the [test gate guide](docs/testing/README.md)
for isolation rules, suite registration and receipt fields.

The [bank router contract](docs/contracts/bank-router.md) defines the initial
shared Markdown layout and its read-only render/validate helper. The full
learning, sharing and installed lifecycle workflows remain under execution.

The [shared lifecycle model](docs/contracts/lifecycle-model.md) supplies the
read-only transition and current-selection decisions used by later operations.

The [developer attribution initializer](docs/contracts/developer-attribution.md)
supports explicit stable-ID creation, adoption and selection before installation.

The [project identity contract](docs/contracts/project-identity.md) describes
stable bank IDs, worktree/clone evidence, remote normalization and explicit binding decisions.

The [observed write contract](docs/contracts/observed-writes.md) defines the
common bank lock, single-file replacement, readback, retry and recovery mechanism.

The [access and remembered-authority contract](docs/contracts/security-authority.md)
explains shared-bank access, developer attribution and the treatment of memory as untrusted data.

The [manual checkpoint workflow](docs/contracts/manual-checkpoint.md) covers
single-file edits, resume, and subsequent validation of external changes.

The [clean-bank usability check](docs/contracts/bank-usability.md) demonstrates
bounded resume from current Markdown with memory-generated state removed.

The [multi-file transaction contract](docs/contracts/bank-transactions.md) defines
visible interruption, external recovery backups and guarded finish/rollback.
