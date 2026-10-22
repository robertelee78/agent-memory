---
id: ADR-0010
status: proposed
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002, ADR-0003]
---

# ADR-0010: Concrete Memory Bank file and scope contract

## Context

Naming Cline's six files is not a complete storage contract. The system must say
where optional topics, episodes, procedures, private overlays, tombstones, and
transactions live; which content is normally shared; and what “usable without a
database” means. Without that contract, a Markdown-first design can become an
unspecified record platform.

## Decision

### Project bank

The default project bank SHALL be `memory-bank/` at the repository root and
SHOULD be committed with the project unless its contents are intentionally
private. Its minimum useful form is:

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

### Non-project scopes

Content that should not be committed to the project SHALL live outside the
repository:

- the principal profile under an XDG-compatible data root keyed by principal;
- a project-personal overlay under the same data root keyed by both principal
  and stable project ID;
- worktree, branch, task, and session observations under an XDG-compatible
  state root until explicitly promoted;
- federation manifests and federation-owned relations in a separate explicit
  federation directory.

The project bank SHALL NOT link to a private overlay by absolute path. An
adapter MAY layer the resolved overlay after scope authorization. Export from an
overlay to the project bank or federation MUST be explicit.

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

### Deletion and tombstones

A deletion tombstone SHALL contain only a random record ID, scope, deletion
time, deletion authority, and non-sensitive reason class. It MUST NOT retain the
deleted content, evidence excerpt, secret, personal data, or a reversible or
low-entropy content hash. File- or section-level deletion that could be recreated
by an optional projection SHALL leave such a tombstone in `history/` or the
authorized private scope.

### Usability definition

A bank is complete and usable without generated state when `MEMORY.md` and its
current routed Markdown targets let a fresh agent resume the project, every
visible statement's scope and current/stale status can be understood, and no
active transaction marker is present. Search indexes, databases, MCP, hooks,
federation, caches, and recovery journals are not part of that definition.

## Consequences

- The common bank remains Cline-shaped and directly editable.
- Private and transient memory do not leak into a Git-tracked project by
  default.
- Checkpoint is intentionally a one-file operation in v1.
- Rich records are available without imposing frontmatter on every paragraph.
- Multi-file operations are detectable and recoverable but not falsely called
  globally atomic.

## Alternatives considered

- **Put every scope under the repository bank:** rejected because personal and
  transient information would be shared accidentally.
- **Require metadata for every prose statement:** rejected because it destroys
  the main usability advantage of a Memory Bank.
- **Claim atomic multi-file replacement:** rejected because ordinary filesystems
  do not provide that transaction across existing files.
