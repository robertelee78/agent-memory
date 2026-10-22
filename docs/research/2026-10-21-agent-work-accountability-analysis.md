# Deep analysis: `agent-work-accountability`

Date: 2026-10-21
Inspected revision: `f31e22418b42ac3fb22cf137ebfc61ea5e9fa321`

## Why this analysis exists

`agent-memory` should be packaged and operated with the same discipline as
`agent-work-accountability` (AWA): one portable skill, progressive instruction
loading, client-neutral behavior, safe installation and updates, a real CLI,
diagnostics, simulation, fault injection, and end-user verification.

This is a transfer analysis, not a claim that memory should copy AWA's GitHub
domain model. Memory has a simpler common path and a stricter requirement that
human-readable files remain useful when supporting machinery is unavailable.

## What was inspected

The review covered the complete instruction tree, CLI, reconciler, schemas,
installer, validators, simulator, tests, and the repository's issue/PR history
for the progressive-skill-tree change. AWA does not currently contain a tracked
corpus of architectural ADR files. Its architectural record is distributed
across its operational guides, code, tests, and the roadmap issue and pull
request that delivered the skill-tree refactor.

The inspected pack contains exactly one `SKILL.md`, 25 `GUIDE.md` files, and
2,676 lines of skill guidance. Its main executable components are:

- `bin/awa`: a roughly 750-line Bash command router and lifecycle surface;
- `skills/github-work-accountability/scripts/reconcile_project.py`: a roughly
  6,550-line reconciliation engine;
- `install.sh`: managed checkout and multi-client installation;
- `scripts/validate_pack.py`: portable-artifact validation;
- `tests/github_sim.py`: a stateful GitHub REST/GraphQL simulator;
- `tests/test_document_projects.py`: protocol and failure-path scenarios.

## Reconstructed architectural decision history

Because AWA has no own ADR corpus, understanding “all ADRs” requires recovering
the decisions and supersessions from its roadmap issues, commits, released
behavior, and deleted research reports. The material sequence is:

| Change | Architectural decision or supersession |
| --- | --- |
| `681440f` | Establish a portable, client-neutral skill. Planning documents own intent; Git, CI, and bound evidence own implementation facts. |
| `9675923` | Make independence from any client, orchestrator, or memory service explicit. |
| `155b6b1` | Add repository-native ADR inspection. Repository policy and Git bytes are authoritative; indexes and memory are discovery aids only. |
| `db771cc` | Harden installation and portable-pack validation. |
| `0c0ae9e` / v0.3 | Replace ad hoc prose/`gh` operations with deterministic inventory → delta → apply → readback reconciliation. |
| `e5a119f` / v0.4 | Replace the initial repository-wide Project with one Project per epic. |
| `98a9c71` / v0.5 | Require an explicit attempt-start event; code activity cannot masquerade as execution ownership. |
| v0.6–v0.7.1 | Add self-update, always-on managed-repository activation, and separation of managed installs from development checkouts. |
| `f7aaf17` / v0.8 | Replace per-epic Projects with one Project per planning document/workstream; add a bounded three-level tree, leaf phases, epic rollups, observed-value guards, and resumable non-destructive migration. |
| Issues #12–#14 | Create replacement views before deletion, use filters valid in GitHub's web UI, and make dry-run deltas exact. |
| Issue #15 / v0.9 | Make Projects private by default and visibility an explicitly protected command. This superseded a proposal to put visibility in the manifest. |
| Issue #16 / v0.10.0 | Make a published full GitHub Release the delivery authority; separate integration from delivery and add release planning, status, attribution, and close operations. |
| Issue #17 | Adopt native blocked-by relations, change-sensitive project statuses, release branches, and reviewed backfill. |
| Issue #18 | Close accepted stories at Release ready so milestone progress is meaningful; `awaiting-release` distinguishes accepted from shipped. This superseded close-only-at-Done. |
| Issues #20–#22 | Add extraction-v3 delivery structure, write-rate preflight, and live view verification. |
| Issues #23–#36 | Tighten merge evidence, protected record lines, evidence listing, Won't-do semantics, batching, stale milestone detection, and epic auto-close while leaving the workstream root open. |
| Issue #38 | Ensure release close skips open, unlanded `next` stories that are outside the release. |
| Issue #40 | Enforce one open attempt per story, add ready selection, and require human disposition after three ended attempts. |
| Issue #45 / v0.10.12 | Deliver the progressive single-skill tree, including the later `SKILL.md` → `GUIDE.md` compatibility correction for recursive OpenCode scanners. |
| Issues #46, #48, #50 | Add ready diagnostics, setup resilience, and idempotent evidence-comment readback after a real CLI file-upload failure. |

Three now-deleted research/RCA documents informed these changes: the initial
GitHub Projects pilot RCA, release-attribution research, and milestone/project
visibility research. Several of their proposals were intentionally superseded:
prereleases do not count as delivery, visibility is not a manifest field,
release notes receive a managed section, and release-less tags are not selected
as the first full release. These resolved decisions are behaviorally clear but
would be easier to audit if retained as first-class ADRs.

## What Project 24 adds to the source audit

[Project 24](https://github.com/users/robertelee78/projects/24) is AWA managing
its own roadmap. At inspection it was public and open, with 26 items: one open
workstream root and 25 completed child stories. The root reported `25/25 Done`;
every item had current source freshness and on-track health. The Project README
contains an immutable AWA marker, repository identity, root issue, skill version,
view numbers, and visibility record.

Its 20-field schema shows why AWA does not overload one “status” value. In
addition to GitHub's built-ins, AWA separately projects Work phase, Health,
Source freshness, Priority, Rank, Section, and Progress. Work phase itself is a
real lifecycle—Backlog, Designing, Ready, Executing, Acceptance, Release ready,
Done, or Won't do—not a synonym for issue open/closed state.

Each roadmap story carries a stable repository-qualified key, outcome,
acceptance criteria, source, delivery boundary, release milestone, integration
commit/PR, and delivered Release. The later stories also record which production
incident or external operating workflow caused the change. This creates a
particularly strong product-development loop:

```text
real failure or observed friction
  → one bounded roadmap story
  → executable acceptance criteria
  → implementation attempt and evidence
  → independent/owner verdict
  → integration commit or pull request
  → published release
  → self-hosted Project projection
```

The roadmap is therefore more than a board. It is the empirical history of
where the protocol was insufficient. The “create views before delete,” web-valid
filter, exact dry run, PR merge-commit API change, write-rate preflight, stale
milestone counter, setup retry, and evidence-readback stories all originate in
specific failures. The tests are expected to preserve those failures as
regressions, not merely exercise abstract features.

Issue #45 and PR #42 make the progressive contract unusually explicit:

- the root routes and defines shared command locations;
- each domain teaches one surface;
- each leaf teaches one operation end to end, including exact commands,
  observed outputs and exit meanings, and raw API fallback;
- every normative instruction retains its actual RFC 2119 strength;
- every quoted result/refusal was obtained by executing the command against the
  simulator or read-only GitHub, not paraphrased from code;
- a section-by-section faithfulness pass mapped every old normative rule to the
  new tree;
- exactly one file remains registerable as a skill across released OpenCode,
  Claude Code, and Codex;
- the complete suite and two independent model reviews were required before the
  owner recorded acceptance.

That last point is important for `agent-memory`: progressive restructuring must
be tested as semantic migration. File existence and link validity alone cannot
prove that a MUST was not dropped, weakened, or stranded behind the wrong route.

## The progressive instruction architecture

AWA's most reusable idea is not merely “split a long skill into files.” It is a
runtime routing contract with four disclosure levels:

```text
discovery description
        ↓
root SKILL.md: universal invariants + route table
        ↓
domain GUIDE.md: rules shared by related operations
        ↓
operation GUIDE.md: one end-to-end procedure + diagnostic escape hatch
```

The root exposes six domains: access, model, plan, board, execute, and release.
The domain guides then route to individual operations such as extract, migrate,
reconcile, attempt, evidence, verdict, or close. An operation guide explicitly
binds back to its domain, so opening a leaf does not accidentally discard the
shared constraints above it. The model guide is conditional: it is loaded when
a question about authority, lifecycle, evidence, or state cannot be resolved by
the active operation.

This is progressive in three independent senses:

1. **Context:** the client initially sees one registered skill and a smaller
   root rather than 26 independently advertised skills.
2. **Decision:** the current runtime state selects the next operation; the
   agent does not ingest every possible transition.
3. **Detail:** normal procedures come first, while raw API or diagnostic steps
   appear only when the high-level command cannot establish the result.

The refactor reduced the always-loaded root from 19,640 to 11,166 bytes while
growing the full operational corpus from approximately 97.8 KiB to 161.4 KiB.
That is the important result: progressive disclosure supported more precise
total guidance with less mandatory context.

The nested files deliberately use `GUIDE.md`, not `SKILL.md`. Released OpenCode
versions recursively registered nested skill files, which caused the earlier
tree to appear as 26 skills. One registered root plus non-registering guides is
therefore both a conceptual design and a cross-client compatibility measure.
Normative instructions retain explicit uppercase
[RFC 2119](https://www.rfc-editor.org/info/rfc2119/) strength across the split;
progressive loading is not allowed to weaken a requirement merely by moving it.

## The operating model underneath the guides

AWA separates authority and state instead of inferring them from activity. Git
planning files, GitHub issues, Projects fields, comments, commits, pull
requests, tags, releases, and local migration state have distinct roles. Work
phase is independent from health, source freshness, attempt ownership, and
delivery evidence.

The central protocol is:

```text
inventory → compute desired delta → dry-run → apply → read back → verified receipt
```

An API success response is not completion. The reconciler verifies the final
issue, field, relationship, view, comment, integration, or release projection
before it records a verified receipt. Durable logical keys are kept separate
from backend IDs. Existing human content is preserved, duplicate keys are
rejected, and managed content is recognizable.

Execution evidence uses marker comments with stable work keys, event types,
references, timestamps, and attempt identities. Evidence gates decide whether
a story may progress. The implementation validates delivery facts against
GitHub rather than trusting a free-form success statement. Attempt records act
as leases, while milestones and native issue relationships project the planning
model into GitHub.

This is the behavioral kernel that should transfer to `agent-memory`: do not
declare a checkpoint, correction, deletion, migration, or index rebuild
successful until the durable files and relevant derived state have been read
back and verified.

## Implementation shape

The documentation is finely decomposed; the code is not. Almost every operation
converges on `reconcile_project.py`. This centralizes protocol behavior but gives
many changes a large blast radius. For `agent-memory`, the progressive guide
tree should be mirrored by clearer internal modules or bounded command handlers,
while schemas and state predicates remain shared.

The reconciler's major responsibilities include:

- manifest parsing and structural validation;
- GitHub repository and project discovery;
- paginated inventory and duplicate detection;
- project schema and option reconciliation;
- issue, milestone, dependency, field, view, and membership projection;
- event-marker parsing and evidence validation;
- attempt acquisition and lifecycle transitions;
- integration, delivery, and release verification;
- rate/write budgets, pending manifests, journals, and receipts;
- local locks and bounded read-after-write retries.

Project writes use an observed/live/desired comparison: a changed field is
written only when the live value still equals the observed value or already
equals the desired value. This avoids clobbering many concurrent edits, although
GitHub's APIs do not provide full compare-and-swap for these operations.

## Install, update, uninstall, and portability

AWA's distribution model is directly reusable:

1. Maintain one managed source checkout at a known local path.
2. Treat the directory under `skills/` as the complete portable artifact.
3. Expose that same artifact to generic Agent Skills, Codex, Claude Code, and
   OpenCode discovery paths through thin links or managed copies.
4. Preflight all target paths and refuse to replace unrelated content.
5. Back up managed replacements and edit client guidance only inside sentinel
   blocks.
6. Record a receipt containing source, revision, mode, targets, and digest.
7. Update only from the recorded remote/ref, from a clean checkout, using a
   fast-forward merge.
8. Report version, revision, digest, source location, and dirty state.

AWA does not currently expose an uninstall command. That is a gap for the new
project, not a behavior to copy. Its ownership information is also split: a
managed-source receipt lives inside the checkout's Git directory, while each
copied skill has its own install receipt; link targets and activation blocks are
reconstructed from conventions rather than one complete installation manifest.

The memory pack should keep the strong contracts and improve these failure modes. AWA's
installer preflights carefully but is not a fully atomic multi-target
transaction if I/O fails during mutation. Its update lock is a directory that
can remain stale after an ungraceful process death. `agent-memory` should stage
an installation, validate it, atomically switch owned targets, retain a rollback
receipt, and use a recoverable lock containing process and start metadata.

AWA update correctly refuses a development checkout, a changed remote, a dirty
managed checkout, an unexpected branch, and non-fast-forward history. It checks
the remote revision without mutating for `--check`, then fetches, fast-forwards,
and reruns the installer. The remaining transactional weakness is ordering: the
managed source can advance before a later client-target activation fails, and
there is no automatic checkout rollback.

Uninstallation is especially important for memory. Runtime and skill removal
must be distinct from deletion of a user's project banks or personal memory.
Ordinary uninstall should remove only manifest-proven owned paths. Durable data
deletion needs a separate previewed purge operation.

## Verification architecture

AWA's test harness invokes the real CLI and reconciler in temporary homes with
a `gh` shim and no live credentials. Its GitHub simulator models repositories,
issues, comments, commits, tags, releases, pull requests, milestones, Projects
GraphQL objects, pagination, API/web filter differences, and mutations. It can
apply a mutation and then drop the response, which proves retry behavior after
the classic “commit succeeded but the client timed out” failure.

Coverage includes migration and crash recovery, stale manifests, idempotent
dry runs, evidence gates, safety, visibility, release, dependencies, epic
closure, attempts, ready selection, installation robustness, evidence readback,
and comment posting. This level of end-user simulation should be copied for
memory file mutations, checkpoint recovery, migration, install/update/uninstall,
deletion propagation, client adapters, and optional index rebuilds.

`tests/run.sh` is an integration gate rather than a loose test collection. It
runs portable-pack validation, installation tests, extraction validation, ADR
inspection, account behavior, and the long document/Project scenario suite.
The scenarios create isolated temporary worlds and invoke the real `awa`
command and reconciler as subprocesses. Assertions target user-visible fields,
views, issue state, evidence, receipts, and errors. This is stronger than unit
testing helper functions because it includes parsing, dispatch, serialization,
transport behavior, readback, and final presentation.

The complete gate was run against the inspected revision during this review. It
passed: portable-pack validation, installer coverage, 15 extraction-validator
tests, 7 ADR-inspector tests, 4 account-helper tests, and 115 document/Project
simulator tests. The longest suite completed in about 550 seconds. The run made
no tracked repository changes.

The simulator deliberately models awkward platform behavior: GraphQL and REST,
pagination, API-vs-web filter differences, separate issue-body/state writes,
Project fields/views, commits, PRs, milestones, Releases, and a mutation that
succeeds server-side before its response is lost. The memory test harness should
copy that philosophy at the filesystem and client-adapter boundaries: inject
failures where real users lose confidence, then retain each incident as a named
end-to-end regression.

The installer suite is similarly product-facing. It verifies all four client
targets, idempotent activation blocks, actionable malformed-block refusal before
any target changes, Bash 3.2 behavior, paths containing spaces, whole-install
conflict preflight, managed one-line/piped installation, link and copy modes,
mode-change refusal without `--replace`, backup preservation, per-copy receipts,
source-development update refusal, update availability/current output, remote
tamper refusal, dirty-checkout refusal, doctor account/scope/remediation output,
and concise restart messaging. `agent-memory` should retain every one of those
classes and add uninstall, transactional rollback, stale-lock recovery, and
proof that no runtime lifecycle operation touches project Memory Banks.

For `agent-memory`, the simulator should additionally exercise:

- two processes editing the same real bank concurrently;
- an atomic file replacement committed before the response is lost;
- stale observed hashes and explicit reconciliation;
- project identity moves and remote changes;
- deleted or revoked content attempting to return from an index;
- poisoned repository content attempting procedural or global promotion;
- a complete loss of every generated database/index followed by recovery from
  the Markdown bank;
- differential checks across Codex, Claude Code, and OpenCode activation.

## Gaps discovered in AWA

The purpose of reuse is to preserve strengths and learn from boundaries, not to
clone every behavior.

### Progressive-pack validation is incomplete

`scripts/validate_pack.py` validates the root skill's frontmatter, name and
description, portability restrictions, and whether relative Markdown targets
exist. It strips fragments before checking files, so it does not verify anchors.
It also does not enforce exactly one registered `SKILL.md`, reachability of all
guides from the root, domain backlinks, absence of routing cycles, leaf-section
contracts, executable bits, or consistency among documented commands, CLI
dispatch, schemas, and version constants.

The memory pack should define its route graph and leaf contract in a
machine-readable manifest and test both documentation and executable surfaces
against it.

### A documented readiness predicate diverges from code

The model guide says ready work requires phase Ready, non-Blocked health, every
predecessor Done, no open attempt, and current source freshness. The
implementation of `show_ready()` checks Ready/open state, health/open blockers,
attempt state, and ended-attempt threshold, but does not enforce source freshness
or require each predecessor to be Done. An accepted or release-ready predecessor
may be closed and therefore disappear as a native blocker before reaching Done.

This illustrates a general design rule for memory: lifecycle predicates should
have one executable definition used by commands, validation, and docs, or a
contract test for every documented condition. Otherwise progressive guidance
can be internally precise and still disagree with runtime behavior.

### Evidence has deliberate trust limits

Marker validation proves syntax, stable-key placement, and referenced GitHub
facts. It does not prove author authorization, signatures, semantic truth, or
acceptance-criterion quality. Reviewer independence is a case-insensitive string
inequality. Unchanged Done/Accepted stories skip comment rereads, so later
comment edits or deletion are discovered only when the story becomes active
again.

Memory provenance needs stronger distinctions among user statements, repository
facts, tool output, assistant inference, and external content. Procedures and
global preferences need an explicit promotion boundary; a syntactically valid
marker is not enough.

### Concurrency is bounded, not fully distributed

AWA uses sorted per-machine `flock` files, last-moment attempt checks, and
observed/live field guards. Cross-machine serialization is not guaranteed.
Comment idempotency is search-then-post without a shared lock, so simultaneous
identical writers can duplicate comments. The test suite's “two agent” cases are
sequential rather than true same-store contention.

Project-bank writes can use atomic replacement plus content hashes and fail
closed on conflicts. Optional shared synchronization should use revision tokens,
append-only idempotent operations, or a real compare-and-swap mechanism rather
than relying on a check immediately before writing.

### Simulator fidelity has a maintenance cost

The GitHub simulator is intentionally a subset and its live behavior calibration
can age. Its own persisted state has no interprocess lock, and POSIX facilities
such as Bash, symlinks, and `flock` are not cross-platform tested. The memory
project should keep its default file protocol small enough to test with actual
filesystem behavior, then use thin contract tests for each client adapter.

### Planning and ADR automation has an agent-mediated seam

AWA's ADR and extraction workflows are precise instructions around deterministic
inspectors, not a complete `awa plan` command path. `inspect_adr.py` reports
declared status and relationships using a deliberately small metadata parser; it
does not resolve relation targets, enforce reciprocal supersession, or establish
semantic adoption. `validate_extraction.py` proves source commits, exact quoted
bytes, keys, topology, and required fields, but semantic coverage still needs
review. The agent performs the write interlock, interpretation, stable-key
matching, and managed-block construction.

That boundary is legitimate, but AWA does not dogfood its ADR path because its
own Project uses an unbound roadmap workstream and the repo has no ADR corpus.
For `agent-memory`, deterministic commands should own atomic file mechanics and
validation while the agent owns semantic judgment; the boundary must be stated
rather than implying full automation.

### Machine-local evidence is not complete shared authority

AWA stores applied manifests and some remembered evidence under local machine
state, while GitHub retains generated record lines and marker comments rather
than the complete canonical manifest. Another machine can therefore know less
and may need an explicit base or the original applying machine. The skill
correctly warns that an empty local cache does not prove evidence is absent.

`agent-memory` should preserve the same distinction more strongly: indexes,
caches, cursors, and recovery journals may be machine-local, but a durable
decision, current project fact, correction, or deletion marker must not exist
only in one machine's auxiliary state.

### Fast discovery and proof use different paths

AWA startup detection uses a cheap read-only GitHub search, while identity-safe
issue operations enumerate paginated open and closed issues and exact-match the
hidden logical key. The first path answers “is this probably managed?”; the
second establishes identity before mutation. Memory should likewise allow a
cheap `MEMORY.md` discovery path, but exact scope, current content hashes, and
authority must be resolved before a consequential write.

## Structure to carry into `agent-memory`

The proposed repository shape is:

```text
agent-memory/
├── README.md
├── install.sh
├── bin/agent-memory
├── skills/agent-memory/
│   ├── SKILL.md
│   ├── model/GUIDE.md
│   ├── access/GUIDE.md
│   ├── lifecycle/GUIDE.md
│   ├── recall/GUIDE.md
│   ├── capture/GUIDE.md
│   ├── govern/GUIDE.md
│   ├── maintain/GUIDE.md
│   ├── evaluate/GUIDE.md
│   ├── templates/
│   ├── schemas/
│   └── scripts/
├── scripts/
│   ├── validate_pack.py
│   └── ...
├── tests/
│   ├── run.sh
│   ├── test_install.py
│   ├── test_memory_bank.py
│   ├── test_concurrency.py
│   └── fixtures/
└── docs/
    ├── adr/
    └── research/
```

The same lifecycle surface should be present: install, uninstall, update,
`update --check`, version, doctor, status, portable validation, temporary-home
end-user tests, source revision and artifact digest reporting, and explicit
restart guidance only when client-visible instructions changed.

## Transfer rules

| Preserve from AWA | Improve for memory |
| --- | --- |
| One registered root skill | Machine-check the one-skill and route invariants |
| Root → domain → operation disclosure | Keep the root smaller than the bank it routes |
| Explicit authority boundaries | Make Markdown files the resilient default surface |
| Inventory → delta → apply → readback | Use atomic file writes and verified checkpoints |
| Stable logical keys | Use content hashes and explicit provenance for records |
| Managed checkout and thin client adapters | Transactional activation and rollback |
| Non-destructive preflight and backups | Make data preservation an uninstall invariant |
| Deterministic simulator and lost-response faults | Add genuine concurrent-writer tests |
| Independent state axes | Generate predicates/docs from one lifecycle definition |
| Diagnostic escape hatches | Never require the diagnostic machinery for basic recall |

The unifying lesson is restraint with rigor: make the common path legible and
small, then give every mutation and optional layer an inspectable contract,
failure mode, repair path, and proof of completion.
