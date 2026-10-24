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
installed lifecycle workflow remains under execution. The [portable installer](docs/contracts/installation.md)
provides preview, owned activation, repeated installation and bootstrap recovery.
The [managed updater](docs/contracts/runtime-updates.md) provides read-only
channel checks, staged fast-forward activation and recoverable retries. The
[runtime uninstaller](docs/contracts/runtime-uninstall.md) preserves project
memory and user edits; a separate inventory-bound, explicitly confirmed purge
deletes only one selected local bank. Deeper diagnostics and the complete
installed workflow retain their separate tasks.

The [shared lifecycle model](docs/contracts/lifecycle-model.md) supplies the
read-only transition and current-selection decisions used by later operations.

The [developer attribution initializer](docs/contracts/developer-attribution.md)
supports explicit stable-ID creation, adoption and selection before installation.

The [project identity contract](docs/contracts/project-identity.md) describes
stable bank IDs, worktree/clone evidence, remote normalization and explicit binding decisions.

The [common scope-binding workflow](docs/contracts/scope-binding.md) combines
developer/project status, previews and verifies registry/bank identity changes,
and guards mediated writes with the resolved scope.

The [evidence-led initializer](docs/contracts/initialization.md) builds a bounded
orientation from existing memory, project sources and AWA work evidence while
preserving older content and surfacing unresolved product questions.

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

The [tombstone contract](docs/contracts/tombstones.md) keeps terminal deletion
markers content-free and guards stale projection rebuilds by stable record ID.

The [memory publication contract](docs/contracts/memory-publication.md) prepares
and lands reviewed memory independently of feature code, with current repository
rules, first-publication support, original-index preservation and actual Git
readback. The full installed workflow remains under execution.

The [landed-memory refresh contract](docs/contracts/memory-refresh.md) reconciles
shared learning into a session without importing code, preserving unpublished
memory, feature history and the original index. Its explicit boundary operation
reads relevant qualified imports and retains honest pending/conflict/recovery states.

The initial [portable skill](skills/agent-memory/SKILL.md) provides an
[offline file workflow](skills/agent-memory/offline/GUIDE.md) for useful manual
setup, inspection, checkpoint and clean resume. Governed initialization and memory exchange have dedicated operation routes; the
complete installed workflow remains under execution.

The [scope policy](skills/agent-memory/scope/GUIDE.md) defines shared repository
knowledge and explicit component, branch, worktree, task and session conditions.
Its read-only decision helper evaluates applicability without claiming access control.

The [authority policy](skills/agent-memory/authority/GUIDE.md) resolves remembered
claims against current sources and routes unresolved contradictions to the
detecting developer. Its evaluator produces read-only correction/revocation candidates.

The [canonical current-bank view](skills/agent-memory/bank/GUIDE.md) inspects
Markdown contexts and local Git facts without generated application state;
tracking is kept distinct from publication and refreshed shared memory.

The [runtime diagnostics](docs/contracts/runtime-diagnostics.md) provide
`agent-memory version`, `status --repo PATH`, and `doctor --repo PATH`.
They report measured runtime identity, quick setup state and deeper ownership/
bank checks without initializing or repairing data. The portable
`scripts/diagnose.py` helper works when the installed wrapper is broken.

The [installed layout contract](docs/contracts/installed-layout.md) ties the
lifecycle commands to all supported discovery targets and separates runtime,
ownership evidence, canonical memory and disposable derived files. Integrated
tests exercise link/copy, custom-only and no-client installations against real
updates, uninstall and preserved project data.

The [typed record contract](skills/agent-memory/records/GUIDE.md) preserves
provenance, conditions, validity and explicit stale state in standalone Markdown.
Its read-only validator also handles optional history and the safe tombstone exception.

The [optional-record loader](skills/agent-memory/layout/GUIDE.md) checks typed
records and bounded topic indexes through one shared algorithm, preserving
provenance and conditions while refusing ambiguous routes or pending observations.

The portable [semantic sharing guide](skills/agent-memory/sharing/semantic/GUIDE.md)
reviews actual local and landed learning before exchange, preserving origins and
conditions and isolating disputed updates. This source-bound review produces
candidate banks; automatic publication and boundary refresh are separate work.
