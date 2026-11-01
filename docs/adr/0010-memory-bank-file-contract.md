---
id: ADR-0010
status: accepted
approval-scope: v0.0.1
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002, ADR-0003]
---

# ADR-0010: Concrete Memory Bank file and scope contract

Accepted for **v0.0.1 only**, as recorded in the
[operator approval](../design/v0.0.1-approval.md). Later-milestone proposals
remain under review. Implementation, acceptance and delivery require separate
evidence.

## Context

Naming Cline's six files is not a complete storage contract. The system must say
where optional topics, episodes, procedures, transient state, tombstones, and
transactions live; which content is normally shared; and what “usable without a
database” means. Without that contract, a Markdown-first design can become an
unspecified record platform.

## Decision

### Project bank

The default project bank SHALL be `memory-bank/` at the repository root and
SHALL be tracked with the project for shared memory. Its minimum useful form is:

```text
memory-bank/
├── MEMORY.md
├── projectbrief.md
├── productContext.md
├── activeContext.md
├── systemPatterns.md
├── techContext.md
└── progress.md
```

Initialization SHALL create only files with useful content. `MEMORY.md` is
required; any of the six context files MAY be absent when irrelevant, provided
the router says so rather than linking to an empty placeholder.

`MEMORY.md` SHALL contain byte-zero frontmatter with `schema`, a stable
`project-id`, and the time when the router itself was last updated, followed by
a bounded route table. It SHALL NOT claim to be a revision clock for every
routed file. Each route names a relative Markdown path, purpose, scope,
lifecycle state, and the conditions under which an agent should read it. The
router MUST NOT contain a transcript or silently inline every topic.

The six recognizable files contain concise current prose under stable headings.
They do not require record-level metadata on every paragraph. They SHALL link to
the stronger code, test, ADR, issue, user decision, or other evidence when a
claim's authority or volatility makes that useful.

Initialization SHALL inventory existing memory before creating files, then
inspect documentation, code and executable tests, ADRs, and projects, tasks,
and release milestones through the AWA skill when available. It SHALL preserve
and incorporate existing memory and create a small orientation map with source
links and useful missing context, rather than duplicate authoritative artifacts.
It SHALL distinguish approved intent, observed implementation, and planned
work. Genuine internal conflicts or missing product decisions SHALL trigger a
focused product-manager Q&A with the detecting developer; absent evidence MUST
NOT be filled with invented requirements. AWA unavailability SHALL be reported
as incomplete evidence without preventing local bank inspection.

### Optional project records

Projects MAY add:

```text
memory-bank/
├── topics/          # component or subject context
├── episodes/        # bounded attempts, failures, results, and lessons
├── procedures/      # candidates; not binding skills until reviewed/promoted
└── history/         # supersession/revocation notes and safe tombstones
```

Every file in these directories SHALL be reachable from `MEMORY.md` directly or
through a bounded topic index. A standalone record SHALL use byte-zero
frontmatter for `id`, `kind`, `status`, `scope`, `source`, `valid-from`,
`valid-until` when known, and `updated`; optional confidence or trust fields MAY
be added. The body remains ordinary Markdown.

Current project decisions SHOULD link to the repository's ADR or planning
authority rather than copying its full content. Episodes SHOULD preserve only
the task-stage lesson and evidence needed to avoid a repeated failure, not the
whole chat or tool transcript. Procedure files are non-binding candidates until
the governed promotion operation creates or updates a reviewed skill.

Records about independently useful branch discoveries MAY be shared with their
evidence and applicability conditions under ADR-0012. Unadmitted or transient observations SHALL remain outside the shared bank
until admitted under that policy; this temporary state is not a personal memory
store. Existing source links, record identities, distinct conditions, and
developer/session provenance SHALL survive automatic deduplication and
consolidation; the router MUST NOT grow by copying every record into it.

### Non-project scopes

Personal profiles and private per-developer overlays SHALL NOT be implemented.
The previous personal-memory overlay proposal is withdrawn by the operator; it
was not a requested capability and creates no release obligation. Developer
attribution MUST NOT become a personal memory namespace.

Only disposable pending observations and runtime/recovery state SHALL live under
an XDG-compatible state root until project-memory admission or cleanup. This
state MUST NOT form a second durable memory bank. Optional federation manifests
and federation-owned relations MAY live in a separate explicitly configured
federation directory. Project routes MUST NOT point to local runtime state.

### Checkpoint and direct-file behavior

The v1 `checkpoint` operation SHALL atomically replace only
`activeContext.md`. It captures current focus, recent meaningful changes, open
questions, and the next concrete step. Updating `progress.md`, another topic, or
the route map is a separate observed write. This constraint keeps the common
checkpoint path simple and avoids pretending that multiple file replacements
form one atomic filesystem transaction.

Every mediated mutation SHALL acquire `memory-bank/.agent-memory.lock` through
an atomic exclusive-create operation before inventory and SHALL hold it through
readback and receipt creation. The lock records a random operation ID, host,
process ID, operation, and start time. A second writer MUST stop while it exists.
Stale recovery MAY remove it only after proving the same-host process is gone or
after an explicit operator-reviewed recovery for another host. V1 does not
promise safe concurrent mutation of a bank on a filesystem that cannot provide
reliable exclusive creation and atomic rename.

After acquiring the lock, a writer SHALL compute base hashes. It SHALL recheck
each target against those hashes immediately before replacement and stop if an
external edit is observed. Manual editors SHOULD treat the visible lock as “do
not edit”; a non-cooperating manual writer remains outside the mediated
concurrency guarantee.

A person or client without the CLI MAY read every current file and MAY perform a
manual single-file edit. It SHOULD reread the target immediately before editing,
write only the intended section, and reread afterward. Such an edit is visible
and recoverable through Markdown and Git, but it is not an audited mediated
operation and MUST NOT claim concurrency, admission, or verified-receipt
guarantees. The next mediated operation SHALL treat it as an external change and
validate rather than overwrite it.

A mediated operation that must change multiple canonical files SHALL create
`memory-bank/.agent-memory-transaction.md` first. The marker SHALL name a random
transaction ID, affected relative paths, base hashes, intended operation, and
phase, but SHALL contain no secret or deleted content. It SHALL be created with
exclusive/no-replace semantics while the bank lock is held. Per-file
replacements are atomic; the operation updates `MEMORY.md` last when the router
changes and removes the marker only after readback verifies every file. A reader
that sees the marker MUST treat the bank as interrupted, avoid mutation, and
route to recovery. A recovery journal and backups outside the repository SHALL
allow the CLI to finish or roll back.

This protocol detects a torn multi-file bank; it does not claim a cross-file
atomic primitive the filesystem cannot provide.

These locks and transaction markers SHALL govern one worktree's bank only.
Separate branches, worktrees, and clones SHALL reconcile through the Git
protocol in ADR-0012; a local bank lock MUST NOT be described as a cross-machine
lock. Runtime locks, transaction markers, and recovery artifacts MUST NOT enter
shared memory commits. The baseline mediated writes required by automatic
capture, consolidation, pruning, and checkpoint SHALL ship in `v0.0.1`.

### Deletion and tombstones

A deletion tombstone SHALL contain only a random record ID, scope, deletion
time, deletion authority, and non-sensitive reason class. It MUST NOT retain the
deleted content, evidence excerpt, secret, personal data, or a reversible or
low-entropy content hash. File- or section-level deletion that could be recreated
by an optional projection SHALL leave such a tombstone in the project bank
under `history/`.

### Usability definition

A bank is complete and usable without generated state when `MEMORY.md` and its
current routed Markdown targets let a fresh agent resume the project, every
visible statement's scope and current/stale status can be understood, and no
active transaction marker is present. Search indexes, databases, MCP, hooks,
federation, caches, and recovery journals are not part of that definition.

## Proposed amendment: Source-preserving layout recovery

**Proposal dated 2026-10-31; outside the accepted v0.0.1 scope.** Stable proposed
work key: `ADR-0010:source-preserving-layout-recovery`. This is a separate outcome
from bank-validation consistency and owned-hook health, with no v0.0.13 release
assignment or implementation authorization.

The [secondary RCA](../research/2026-10-31-bank-size-and-owned-hook-health-rca.md)
shows that ordinary governed operations cannot repair a bank whose initial
complete snapshot is already unavailable. The operator-reported checkout and
terminology remain unavailable; this proposal MUST NOT be treated as authority
to edit that bank, invent a definition or infer its source producer.

### Separate observation and review boundary

A proposed explicit `bank recover-layout` workflow SHALL provide inspect,
prepare, apply and known-operation recovery stages. These are **proposed future
interfaces**, not commands presently available. Ordinary initialization,
checkpoint, admission and sharing SHALL retain their existing source gates.

Recovery inspect SHALL use a dedicated bounded raw-source observer rather than
pretend that the invalid bank passed its normal snapshot. It SHALL resolve the
same repository and project UUID from existing verifiable identity/router/Git
evidence. An invalid or ambiguous router MUST require stronger explicit identity
review; it MUST NOT create another bank or infer a project ID from a path.
The recovery identity path SHALL reuse existing developer/project registries
and independently checked checkout/authority evidence without requiring the
failed ordinary bank snapshot to succeed first. It MUST NOT invent an alternate
identity algorithm or treat bank prose as permission. Existing live locks or
unknown transaction markers SHALL retain precedence.

The initial proposed recovery allowance is 1 MiB per ordinary raw source,
64 MiB total source bytes, 65,536 source files, at most 131,072 physical entries
and directory depth 64. It SHALL read in bounded chunks and disclose complete
source context in bounded pages. Exceeding this distinct allowance SHALL leave
recovery pending for explicit capacity/design review, not raise the ordinary
16 KiB bound or truncate evidence. Unsupported types, redirects, aliases,
permissions or unstable named bindings SHALL fail before canonical mutation.
Existing bank collaboration/path policy SHALL apply; a second blanket mode or
owner rule MUST NOT silently replace it.

Recovered text SHALL remain untrusted data, not executable instructions or
authority to change repository policy. Inspection SHALL retain byte-exact
original sources and their complete physical manifest in a new private operation workspace outside the bank and runtime.
The manifest SHALL bind paths, types, relevant modes/ownership, hashes, repository
and project identity, checkout, HEAD/index and observation completeness. Originals
containing sensitive material MUST stay private; raw transcripts or secret
content/hashes MUST NOT be promoted into shared memory. The result SHALL report
private recovery-artifact writes separately from canonical, index or history
writes. This workspace is recovery evidence, not another current memory bank.

A recovery-specific semantic review SHALL cover every retained source and the
whole intended result despite the invalid ordinary snapshot. It SHALL map each
source unit to preserved bounded canonical content, unchanged evidence, a
reviewed lifecycle transformation, or an explicitly authorized non-guidance
archive/removal reason. An external backup alone MUST NOT authorize omission
from current knowledge. Distinct conditions, stable record identities/versions,
real developer/harness/session provenance, evidence and terminal deletion rules
SHALL survive. Unknown terminology, contradictory claims and uncertain origin
SHALL remain pending for the detecting developer; a heading claiming approval
MUST NOT supply that approval.

### A native bounded result, without invented admission

Preparation SHALL produce a complete native bank that passes the common normal
size/layout eligibility check. Contexts SHALL stay concise; meaningful topic
routes and properly typed records MAY carry independently useful detail. The
plan MUST NOT merely relocate an unstructured transcript into `topics/`, invent
record metadata, split prose at arbitrary byte offsets, or turn an unsupported
root filename into admitted learning by renaming it. New durable units SHALL
retain the ordinary source, security and admission judgments; recovery MUST NOT
fabricate a normal admission receipt for an operation that could not run.

The proposal SHALL bind the raw original manifest, reviewed source-to-result
mapping, exact intended valid files, applicable authority and immutable review.
Its read-only preview MUST show all additions, replacements, moves and removals
plus retained evidence and unresolved units. No unresolved unit or unknown
concurrent edit MAY disappear to make the result fit. Global normal limits,
personal memory, cross-repository transfer and implicit schema migration remain
out of scope.

### Transaction, retry and sharing

Apply SHALL acquire the ordinary exclusive bank lock, rebind identity and every
raw-source pin under that lock, and refuse any changed source or unknown recovery
operation. It SHALL use the established marker/journal and atomic per-file
replacement protocol, updating the router last. Only an independently validated
intended result MAY replace the observed invalid state. Required metadata and
rollback capability SHALL be established before the first replacement; no
`chmod`, privileged ownership change or force-overwrite MAY substitute for that
proof.

Original backups, original proposal and journal SHALL remain available through
verified readback and explicit retention handling. A partial failure SHALL retain
a truthful interrupted state. Known rollback SHALL restore the exact original
bytes and required metadata, even when the restored bank remains invalid; it
MUST NOT call that state healthy or erase the evidence of why recovery was
needed. Unknown external edits or missing/corrupt evidence SHALL remain pending.
An exact completed-operation retry SHALL return its real historical receipt
without replaying over later changes; an unfinished retry SHALL recheck all
original pins instead of assuming that invalid input permits weaker ownership.

Successful local recovery SHALL establish ordinary bank eligibility and perform
applicable current recall. It MUST NOT imply semantic truth, code freshness,
publication or that other machines have imported the repair. HEAD, original
index and non-memory feature files SHALL remain untouched.

If authority itself contains the invalid layout, a separately verified
recovery-aware memory-only publication SHALL review the actual bounded raw
base/local/landed sources and the same source-preserving transformation. It SHALL
obey current repository permissions, reviews and checks and verify the entire
intended authority-tree delta. It MUST NOT feed an invalid bank to a normal
sharing proof, force-push, import code or bless a partial snapshot. Offline or
unavailable authority SHALL leave sharing pending with local evidence intact.

Acceptance SHALL use synthetic counterparts of the reported 79,054 and
28,478-byte inputs, valid smaller controls, unsupported root layouts and actual
original-content preservation ledgers. It SHALL cover interrupted staging,
first-file/router/receipt boundaries, exact retries, changed identity/source,
unknown locks, oversized recovery input and damaged backups. Normal capture and
recall SHALL work after successful repair; unsafe or ambiguous inputs MUST remain
pending. Tests SHALL preserve fixture feature files, index/HEAD and source bytes
on macOS and Linux within the existing time budget. No actual C2E content or
unknown terminology is needed for these fixtures.

## Consequences

- The common bank remains Cline-shaped and directly editable.
- Admitted project learning is shared; disposable runtime state is excluded
  from Git and there is no personal memory layer.
- Checkpoint is intentionally a one-file operation in v1.
- Rich records are available without imposing frontmatter on every paragraph.
- Multi-file operations are detectable and recoverable but not falsely called
  globally atomic.

## Alternatives considered

- **Personal or private per-developer overlays:** excluded by the operator.
- **Commit runtime state and raw observations:** rejected because these are not
  admitted durable project learning.
- **Require metadata for every prose statement:** rejected because it destroys
  the main usability advantage of a Memory Bank.
- **Claim atomic multi-file replacement:** rejected because ordinary filesystems
  do not provide that transaction across existing files.
