---
id: ADR-0012
status: accepted
approval-scope: v0.0.1
date: 2026-10-22
depends-on: [ADR-0001, ADR-0002, ADR-0005, ADR-0010, ADR-0011]
---

# ADR-0012: Git-shared memory across worktrees, machines, and developers

Accepted for **v0.0.1 only**, as recorded in the
[operator approval](../design/v0.0.1-approval.md). Later-milestone proposals
remain under review. Implementation, acceptance and delivery require separate
evidence.

## Context

The primary workflow uses Claude Code, Codex, OpenCode, and custom harnesses,
with concurrent sessions in separate worktrees and branches across two to five
computers and two developers. A shared mutable vector store previously failed
under concurrency and did not carry useful memory reliably between machines.

The project already uses GitHub for code and architectural intent, AWA for work
state and delivery evidence, and Vox for live coordination. Memory needs a
deliberate Git publication and refresh path while each session keeps its normal
code-branch workflow. Uncommitted or unlanded learning is not expected to be
visible elsewhere. A clean textual merge alone cannot decide whether two claims
mean the same thing or contradict each other.

## Decision

**Repository boundary.** Shared project memory SHALL be tracked as ordinary
Markdown under the configured bank root, defaulting to `memory-bank/`. The
shared authority branch SHALL be the repository's configured default branch
(`main` in this repository). Only admitted project memory SHALL enter this
channel. Raw transcripts, credentials, runtime locks,
transaction markers, caches, and recovery state MUST NOT be committed by this
workflow. AWA SHALL remain authoritative for work state and delivery; Vox SHALL
remain authoritative for live coordination. Their absence MUST NOT prevent
local memory use, and memory MUST NOT become a duplicate tracker or message bus.

**Automatic publication.** The companion skill SHALL automatically prepare,
commit, push, and land admitted memory-only changes through the repository's
existing permissions, checks, review requirements, and branch rules. A required
human review SHALL leave publication pending rather than be bypassed. Publication
SHALL use the latest shared memory as its base and MUST NOT bring feature code,
unrelated staged changes, or feature-branch ancestry into the authority branch.
Independently useful discoveries from unfinished work MAY land with explicit
evidence and applicability conditions. Before landing, the skill SHALL search
and read plausible existing matches again, combine equivalent learning, and
check for semantic contradictions. A changed authority-branch head SHALL trigger
reconciliation and validation again before retry; publication MUST NOT force
push over another writer. Success SHALL be reported only after readback proves
the intended memory delta landed.

**Turn-boundary refresh.** At each turn boundary, the skill SHALL check the
authority branch for landed memory changes and reconcile only memory paths into
the session's branch. It SHALL use a three-way comparison of the previously
observed shared memory, current local memory, and newly landed memory, preserving
unpublished local learning. It MUST NOT replace the whole directory with a
snapshot, rebase the feature branch, import non-memory code changes, or alter
unrelated index entries and working files. Relevant imported knowledge SHALL be
read with its provenance and applicability conditions. Refresh MUST NOT imply
that the branch's implementation now matches the authority branch. Invocation
SHALL work through a harness-neutral skill and explicit CLI helper, without
interrupting ongoing work or requiring client hooks, MCP, or a daemon.

**Semantic reconciliation and clarification.** Compatible edits SHALL combine
even when they touch the same memory file. Same-concept duplicates SHALL
consolidate while retaining distinct conditions, evidence, provenance, and
useful correction history. The skill SHALL inspect meaning even when Git reports
no textual conflict, including claims in different files. For incompatible
claims about the same subject under overlapping conditions, such as yellow and
blue for the same wall, the detecting session SHALL show both claims and their
evidence to its current developer. If another developer originated a claim, the
prompt SHALL identify that attribution and recommend reaching out for
clarification as needed; it MUST NOT message another person automatically.
Only the disputed update and work depending on it SHALL pause. Independent work
and unrelated memory updates SHALL continue. The skill MUST NOT select a winner
by landing order, recency, or developer identity. Resolution SHALL record the
clarifying decision and safely correct or supersede affected guidance.

**Retry, deletion, and ordinary merges.** Publication and refresh SHALL survive
offline periods, competing writers, interruption, and lost push/merge responses
without corrupting readable memory. Local capture SHALL remain useful while
remote work is pending; the system SHALL report pending or stale sharing rather
than claim freshness. Reruns SHALL inventory current Git state before applying
missing changes and MUST NOT duplicate successful publications. Tracking cursors
and retry state SHALL remain disposable local state: losing them MAY require
reconciliation but MUST NOT lose the canonical bank. Deletes and tombstones
SHALL participate in reconciliation so an older branch cannot silently restore
pruned, revoked, or deleted guidance. A later ordinary merge or rebase of code
SHALL reconcile previously imported memory without duplicating or reversing it.

**First-release acceptance and quiet operation.** This complete sharing loop
SHALL ship in `v0.0.1`. An isolated acceptance scenario SHALL demonstrate that
one session publishes durable learning from unfinished feature work and a
different harness/session on another clone recalls it at its next successful
boundary refresh, with code history and non-memory changes unaffected. It SHALL
also cover two developer identities, simultaneous compatible edits, duplicate
learning, semantic conflicts, conditional applicability, pruning, offline retry,
and a later normal code merge. Routine successful maintenance SHALL be quiet.
Conflicts, failures, and consequential corrections SHALL be surfaced promptly
at a boundary. No acceptance test SHALL require real credentials or change live
client configuration, repositories, or memory.

## Consequences

- Shared memory is a special part of the repository with its own publication
  cadence; ordinary code integration stays under each session's existing rules.
- A local file lock protects one worktree, while Git reconciliation protects
  exchange among independent worktrees and machines.
- Semantic review is an agent responsibility supported by evidence and tests;
  the deterministic helper must not promise perfect contradiction detection.
- The first release needs real multi-clone merge and recovery tests, not only
  installation and Markdown templates.

## Alternatives considered

- **A required shared vector database:** rejected because readable memory and
  cross-machine continuity must survive loss of optional generated state.
- **A whole-branch pull at each boundary:** rejected because memory refresh must
  not import unrelated code or change the session's integration policy.
- **Overwrite local memory with the latest snapshot:** rejected because this
  discards unpublished local learning and can restore deleted guidance.
- **Last writer wins:** rejected because commit order does not resolve truth.
- **Vox as durable memory or AWA as a memory store:** rejected because live
  coordination, work evidence, and durable project learning have different jobs.
