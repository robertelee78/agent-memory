# agent-memory

A portable, local-first Memory Bank for coding agents.

## v0.0.1

One repository owns one shared Markdown memory bank. The skill captures durable
learning, reads existing memory before adding it, combines duplicates without
losing conditions or evidence, and recalls relevant knowledge. Memory can land
independently of unfinished feature code and refresh into other sessions,
worktrees and clones through the repository's existing Git rules. Contradictions
go to the detecting developer for clarification.

See the [v0.0.1 release guide](docs/releases/v0.0.1.md) for the supported workflow,
compatibility and known limits. The [published release](https://github.com/robertelee78/agent-memory/releases/tag/v0.0.1)
holds the release's source and validation evidence.

## Install and first use

Start from a trusted complete checkout with Python 3.11+ and Git. Preview before
activating the default managed `main` channel:

```sh
./install.sh --check --mode copy
```

Inspect the source and every proposed target, then apply the same configuration:

```sh
./install.sh --mode copy
```

Defaults include generic Agent Skills, Codex, Claude and OpenCode discovery.
Use `--presets none` and repeated `--target-dir` arguments for custom discovery.
The receipt identifies `bin_dir`; add that directory to your shell's `PATH` if
needed. Installation needs neither a developer identity nor an initialized bank.
Follow the [installation guide](skills/agent-memory/lifecycle/install/GUIDE.md)
for ownership, recovery and explicit local/development sources. Development
sources are separate from the managed channel and are not updated by the updater.

Ask your coding agent to use the installed `agent-memory` skill in the target
repository. It routes explicit developer/project setup, evidence-led bank
initialization, qualified recall and the learning loop. At natural turn
boundaries it refreshes landed memory, compares new learning with existing
memory, and publishes eligible memory through current repository rules. Load
the skill in each session; v0.0.3 hooks are planned to cover forgotten invocation.
The [common CLI guide](skills/agent-memory/maintain/cli/GUIDE.md) documents the
same public helpers and installed aliases.

For an existing managed installation, preview with `agent-memory update --check`
before `agent-memory update`. Preview removal with `agent-memory uninstall --check`
before `agent-memory uninstall`. Ordinary uninstall preserves project memory and
user edits; explicit bank deletion has a separate confirmed purge operation.
Keep a trusted complete package available for recovery after wrapper removal.
See [update](skills/agent-memory/lifecycle/update/GUIDE.md),
[uninstall](skills/agent-memory/lifecycle/uninstall/GUIDE.md) and
[diagnostics](skills/agent-memory/lifecycle/diagnose/GUIDE.md).

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

The design draws on primary research, current production systems,
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
Projects. GitHub records the v0.0.1 tasks' design approval, execution, independent
acceptance and delivery. Their
[ordered queue](docs/design/v0.0.1-work.json)
preserves the existing task identities. Prerequisite delivery is reviewed merge
to main, with the complete release tracked by its final publication gate.

The refined `v0.0.1` scope includes automatic durable capture, deduplication,
consolidation, relevant recall, and evidence-based pruning, plus
[memory-only Git publication and turn-boundary refresh](docs/adr/0012-git-shared-memory.md)
across worktrees, computers, and developers. Code integration keeps its normal
workflow. Later release milestones remain planning proposals for refinement.

## Development checks

The [installed common CLI](skills/agent-memory/maintain/cli/GUIDE.md) exposes
identity, initialization, qualified recall, capture/admission, checkpointing,
governance, consolidation and memory-only publication/refresh through the same
portable helpers. The skill performs the evidence and semantic judgments at
natural turn boundaries; `agent-memory boundary` invokes the existing refresh
protocol. The [file-only workflow](skills/agent-memory/offline/GUIDE.md) remains
available when the command and helpers are absent.

Run `scripts/test.sh` from a checkout with Python 3.11+ and Git. It runs the
offline public-interface suites in disposable test worlds. The registry declares
116 mandatory suites and explicitly excludes optional future stores.
`scripts/test.sh --release` rejects incomplete release coverage. See the [test gate guide](docs/testing/README.md)
for isolation rules, suite registration and receipt fields.
The [portable lifecycle matrix](docs/contracts/portable-lifecycle.md) connects
the complete lifecycle requirements to real installed-command and fault cases.

The [bank router contract](docs/contracts/bank-router.md) defines the initial
shared Markdown layout and its read-only render/validate helper. The [portable installer](docs/contracts/installation.md)
provides preview, owned activation, repeated installation and bootstrap recovery.
The [managed updater](docs/contracts/runtime-updates.md) provides read-only
channel checks, staged fast-forward activation and recoverable retries. The
[runtime uninstaller](docs/contracts/runtime-uninstall.md) preserves project
memory and user edits; a separate inventory-bound, explicitly confirmed purge
deletes only one selected local bank. Read-only `version`, `status` and `doctor`
commands expose runtime identity and bank health. The complete portable lifecycle
matrix is included in the mandatory release gate.

The [shared lifecycle model](docs/contracts/lifecycle-model.md) supplies the
read-only transition and current-selection decisions used by later operations.

The [developer attribution initializer](docs/contracts/developer-attribution.md)
supports explicit stable-ID creation, adoption and selection for governed memory
operations; runtime installation is independent of identity setup.

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
The [concurrent-writer contract](docs/contracts/concurrent-bank-writers.md) tests
actual competing processes, stale observations, atomic visibility and recovery,
with distinct-worktree locks and memory-only publication kept separate.

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
readback through the installed common CLI.

The [landed-memory refresh contract](docs/contracts/memory-refresh.md) reconciles
shared learning into a session without importing code, preserving unpublished
memory, feature history and the original index. Its explicit boundary operation
reads relevant qualified imports and retains honest pending/conflict/recovery states.

The initial [portable skill](skills/agent-memory/SKILL.md) provides an
[offline file workflow](skills/agent-memory/offline/GUIDE.md) for useful manual
setup, inspection, checkpoint and clean resume. Governed initialization and
memory exchange have dedicated operation routes in the installed workflow.

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

The [lifecycle fault harness](docs/contracts/lifecycle-faults.md) enumerates real
before/after write interruptions, verified commits with lost responses, and
recovery/retry checks. Its isolated suite workers collect their process groups
before cleanup; the complete release gate requires every registered mandatory
suite and complete interruption/recovery/retry proofs.

The [typed record contract](skills/agent-memory/records/GUIDE.md) preserves
provenance, conditions, validity and explicit stale state in standalone Markdown.
Its read-only validator also handles optional history and the safe tombstone exception.

The [optional-record loader](skills/agent-memory/layout/GUIDE.md) checks typed
records and bounded topic indexes through one shared algorithm, preserving
provenance and conditions while refusing ambiguous routes or pending observations.

The portable [semantic sharing guide](skills/agent-memory/sharing/semantic/GUIDE.md)
reviews actual local and landed learning before exchange, preserving origins and
conditions and isolating disputed updates. This source-bound review produces
candidate banks; the publication and boundary refresh operations consume that
review under their own validation and readback requirements.
