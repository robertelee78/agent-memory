# agent-memory

A portable, local-first Memory Bank for coding agents.

## v0.0.6

One repository owns one shared Markdown memory bank. The skill captures durable
learning, reads existing memory before adding it, combines duplicates without
losing conditions or evidence, and recalls relevant knowledge. Memory can land
independently of unfinished feature code and refresh into other sessions,
worktrees and clones through the repository's existing Git rules. Contradictions
go to the detecting developer for clarification.

The [v0.0.6 release guide](docs/releases/v0.0.6.md) covers updates, repair,
and their current verification limits.
[Published releases](https://github.com/robertelee78/agent-memory/releases)
hold the immutable source and actual qualification evidence for each version.

## Install and first use

### Install

Install on macOS or Linux with one command, just like AWA:

```sh
curl -fsSL https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh | bash
```

The default command verifies delivery with a pinned publisher key.
Requires Bash 3.2+, Python 3.11+, Git, curl and OpenSSH `ssh-keygen` with SSH
signature support. No checkout, GitHub authentication, developer identity or
initialized memory bank is needed. The bootstrap verifies the signed release
manifest and portable archive before
running candidate code, delegates to the owned installer, and removes private
staging. The installed runtime lives in its own managed location; the archive
contains no development memory from this repository. The fetched bootstrap itself
remains the initial HTTPS trust anchor.
Restart running agent sessions after installation so they load the skill.

Turn-boundary adapters supply the shared skill instruction in Claude, Codex and
OpenCode. Codex may require you to review and trust the Agent Memory hook in
`/hooks`; configuration alone does not prove a hook ran or that memory was
captured. Conflicts and unavailable publication remain pending for the session.
Keep Python 3.11+ available as `python3` on `PATH` when running `am`.
Re-run the default one-line command to update an existing managed installation,
or use `am update`.

When upgrading from v0.0.3 with `am update`, run `am update` a second time
to activate the new owned client hooks, then restart the agent sessions. The
first update runs the old updater, which cannot install targets introduced by
the new version. Check client hook trust and use `am doctor` if activation fails.

For an optional preview or custom install options, pass arguments to Bash:

```sh
curl -fsSL https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh | bash -s -- --check
```

Remove `--check` to apply. From an existing complete checkout,
`./install.sh` invokes the same installer.

Defaults include generic Agent Skills, Codex, Claude and OpenCode discovery.
Use `--presets none` and repeated `--target-dir` arguments for custom discovery.
The primary command is `am`; `agent-memory` remains a compatibility command.
Both are installed in `~/.local/bin` by default. If your shell cannot find them,
add that directory to `PATH` (and to your shell startup file to persist it):

```sh
export PATH="$HOME/.local/bin:$PATH"
am --version
```

If you choose a custom `--bin-dir`, use the receipt's `bin_dir` instead.
Follow the [installation guide](skills/agent-memory/lifecycle/install/GUIDE.md)
for ownership, recovery and explicit local/development sources. Development
sources are separate from the managed channel and are not updated by the updater.

### First use in a repository

Ask your coding agent to use the installed `agent-memory` skill in the target
repository. It routes explicit developer/project setup, evidence-led bank
initialization, qualified recall and the learning loop. At natural turn
boundaries it refreshes landed memory, compares new learning with existing
memory, and publishes eligible memory through current repository rules. The installed adapters provide turn-boundary guidance; the agent must still
complete the governed workflow, and denied permissions or failures remain pending.
The [common CLI guide](skills/agent-memory/maintain/cli/GUIDE.md) documents the
same public helpers and installed aliases.

Install once per computer. Each repository keeps its own Git-tracked bank and
explicit project binding; it does not need a separate runtime installation.
Cloning this development repository includes its project memory. Installing the
portable runtime does not copy, initialize or bind that development bank for a
consumer repository.

### Semantic recall

Ask your coding agent a plain-language question about the project. The skill
interprets meaning, navigates bounded topic maps through the CLI and reads
qualified current records, preserving conditions and source evidence. This
workflow supports Claude, Codex, OpenCode and custom harnesses without requiring
a vector service. Direct terminal `am search` remains literal;
[standalone terminal semantic search](https://github.com/robertelee78/agent-memory/issues/200)
is tracked for a later release.

To inspect setup and health, replace `/path/to/project` with your repository or
worktree path. Both commands are read-only; missing identity or bank setup is
reported separately from installation health:

```sh
am status --repo /path/to/project
am doctor --repo /path/to/project
```

To repair problems that can be safely verified, run:

```sh
am doctor --fix --repo /path/to/project
```

The repair reports what it changed and what still needs attention. It preserves
unrelated user files and memory; a verified interrupted bank transaction may be
converged to its recorded outcome. It does not guess when ownership or a
recovery outcome is uncertain.

### Update

Update the managed runtime with one command:

```sh
am update
```

If the installed release and owned targets are current, the command reports
that `am` is up to date without scanning unrelated repositories. If an owned
client target needs reconciliation, it repairs that target when safe and reports
whether clients need restarting.

To preview first:

```sh
am update --check
```

The signed default follows the latest stable GitHub Release. It verifies the
pinned publisher signature and complete artifact identity before activation;
unsigned downloads are refused. The managed runtime is independent of your
project checkouts. Explicit custom Git sources retain their recorded branch and
fast-forward policy; development sources remain operator-selected. See the [update guide](skills/agent-memory/lifecycle/update/GUIDE.md)
for source checks, retries and recovery.

If you installed before `am` was introduced, run this one-time transition:

```sh
agent-memory update && agent-memory install
```

The shipped older updater upgrades the runtime first; the new installer then
adds the owned `am` command. Afterward, use `am update`. An unrelated existing
`am` is preserved and reported instead of replaced.

An already-running v0.0.2 updater cannot authenticate its first transition.
The new updater reports that activation as legacy unsigned; the next verified
update adopts the signed release. Re-running the trusted one-line installer
verifies its candidate before activation.

The repeated default one-line installer also performs the guarded update and
refreshes owned commands. Default install/update/uninstall and diagnostic output
is readable; add `--json` for complete machine results or `--verbose` for detail.
Use `am`, `am help` or `am --help` for help. `am --version`, `am -V` and
`am version` report the same executing version, revision and source.

### Uninstall

Uninstall the owned runtime and discovery entries with one command:

```sh
am uninstall
```

To preview first:

```sh
am uninstall --check
```

Ordinary uninstall preserves project memory and user edits; modified owned
targets are reported and preserved. Explicit bank deletion has a separate
confirmed purge operation. See the [uninstall guide](skills/agent-memory/lifecycle/uninstall/GUIDE.md).

### Diagnose and recover

Use `am version`, `am status` and `am doctor`
for diagnostics. If a runtime transaction was interrupted, use
`am recover` and inspect its result before retrying the operation.

If the installed command is unavailable, run recovery from a trusted complete
checkout kept outside the managed installation targets:

```sh
cd /path/to/agent-memory
python3 -I skills/agent-memory/scripts/installer.py recover
```

See the [diagnostics guide](skills/agent-memory/lifecycle/diagnose/GUIDE.md)
for interpreting findings and choosing a repair.

## Mission

Take what is good about
[Cline's Memory Bank](https://docs.cline.bot/best-practices/memory-bank)—small,
ordinary Markdown files; an obvious project model; explicit checkpoint and
resume; human inspectability; and no service dependency—package it with the
progressive skill and lifecycle discipline of `agent-work-accountability`, and
improve it without losing that simplicity.

The common path MUST remain useful with files, Git, and the installed skill
alone, with a deterministic CLI for governed operations. MCP is outside planned
scope. Turn-boundary adapters direct supported agents to the shared capture and
refresh workflow, without bypassing harness permissions. Any optional search projection must
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
tasks. The operator also approved v0.0.4 turn-boundary hook work; other later
proposals remain under review. Design approval does not claim that the
implementation already exists.

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
workflow. Release milestones show the current approved and planned work.

## Development checks

The [installed common CLI](skills/agent-memory/maintain/cli/GUIDE.md) exposes
identity, initialization, qualified recall, capture/admission, checkpointing,
governance, consolidation and memory-only publication/refresh through the same
portable helpers. The skill performs the evidence and semantic judgments at
natural turn boundaries; `am boundary` invokes the existing refresh
protocol. The [file-only workflow](skills/agent-memory/offline/GUIDE.md) remains
available when the command and helpers are absent.

Run focused public-interface checks from a checkout with Python 3.11+ and Git.
Use disposable test worlds and retain the source revision and results. Routine
qualification runs locally on the available macOS and Linux machines; each test
invocation should reasonably finish within 10–15 minutes. Run a broader or
hosted matrix only when its coverage is needed.

`scripts/test.sh` runs the registered offline suites; `scripts/test.sh --release`
applies the historical complete gate. These full commands can take
longer than the routine time limit and are not required for every release. See
the [test gate guide](docs/testing/README.md) for their isolation and receipt
format. Release evidence must identify the checks actually run and any limits.
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
commands expose runtime identity and bank health. The portable lifecycle matrix
provides broader coverage when that qualification is needed.

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

The [lifecycle security contract](docs/contracts/lifecycle-security.md) separates
publisher authentication from integrity, ownership, compatibility and recovery.
Default release sources verify the pinned publisher signature. Legacy managed
Git and explicit development/custom sources retain their unsigned trust boundary;
a digest or mechanical receipt alone is not a signature.

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
`am version`, `status --repo PATH`, and `doctor --repo PATH`.
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
before cleanup. Its full gate requires every registered suite and complete
interruption/recovery/retry proofs; focused runs report only the cases exercised.

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
