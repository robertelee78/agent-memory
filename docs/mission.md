# Mission and product doctrine

## Mission

Take what is good about Cline's Memory Bank, package it with the progressive
skill and operational discipline of `agent-work-accountability`, and improve it
without turning it into a brittle memory platform.

The first users work across Claude Code, Codex, OpenCode, and custom harnesses,
with concurrent sessions in separate branches/worktrees, two to five computers,
and two collaborating developers. GitHub carries shared repository knowledge,
code, and ADRs; AWA owns work state and delivery evidence; Vox owns live agent
coordination. `agent-memory` owns durable learning and relevant context. The
first release MUST preserve useful learning through ordinary Git publication,
merge, and recall, without a shared mutable database.

`agent-memory` exists so a fresh coding agent can enter a project and quickly
understand:

- what the project is and why it exists;
- how it is built and the patterns it follows;
- what is true now, with links to stronger evidence;
- what the current focus and next step are;
- what was tried, learned, decided, or invalidated;
- which uncertainty should be checked rather than repeated as fact.

## What must survive from Cline

1. **Ordinary Markdown is the product.** A person can inspect, edit, review,
   copy, diff, and recover the bank without specialized software.
2. **The project has an obvious shape.** Brief, product context, active context,
   system patterns, technical context, and progress answer different questions.
3. **Checkpoint and resume are explicit.** Updating memory at a meaningful
   boundary and starting fresh from it are understandable operations.
4. **It is client-independent.** Any capable coding agent can use the files.
5. **It grows with the project.** Initialization creates only useful content;
   new topics appear when actual complexity requires them.
6. **Maintenance is accountable.** The agent captures durable learning
   automatically and quietly; conflicts, failures, and consequential corrections
   are surfaced promptly. Accumulating chat is not understanding.

## What AWA contributes

1. One registered portable skill with root → domain → operation disclosure.
2. One shared semantic model and explicit authority boundaries.
3. Exact user-facing operations and RFC 2119 requirement strength, with
   diagnostics only when needed.
4. Idempotent, non-destructive install/update behavior and recovery patterns;
   `agent-memory` adds the uninstall contract that AWA lacks.
5. Version, source revision, artifact digest, doctor, and status surfaces.
6. Inventory → plan → apply → readback → verified receipt for mutations.
7. Temporary-home, real-CLI tests with fault injection and lost-response cases.
8. Thin Codex, Claude Code, OpenCode, and generic Agent Skills adapters over one
   client-neutral artifact.

## What should improve

- Add a bounded `MEMORY.md` route map so every session need not read every file.
- Distinguish current facts, active context, decisions, episodes, feedback, and
  procedural candidates instead of mixing them into one growing narrative.
- Link claims to source code, tests, ADRs, user decisions, or tool evidence when
  those are the actual authority.
- Mark uncertainty, validity, correction, supersession, and revocation
  explicitly when they matter.
- Make checkpoints atomic, conflict-aware, idempotent, and verifiable.
- Keep shared project memory separate from runtime state and optional explicit
  cross-project federation. Personal memory is outside product scope.
- Evaluate whether memory improves executable coding outcomes and prevents
  repeated failures, not merely whether text can be recalled.
- Treat retrieved content as data; only reviewed procedural promotion can make
  a memory instructional.

## Complexity ladder

Complexity is earned one layer at a time:

```text
Level 0  Markdown Memory Bank + portable skill
Level 1  lifecycle CLI, validation, atomic checkpoints, doctor/status
Level 2  disposable exact/full-text/semantic indexes when scale requires them
Level 3  explicit federation and a disposable knowledge-graph projection
```

Each level must remain useful when every level above it is removed. One-off
projects should normally stop at Level 0 or 1. A knowledge graph belongs to an
actively maintained ecosystem of related projects, not to the default project
template.

## First-release non-goals

The first release is not:

- a transcript archive;
- “save everything and vector-search it”;
- a required database or background daemon;
- a cloud memory service;
- an automatic global knowledge graph;
- a replacement for source code, tests, ADRs, Git, issue tracking, CI, or user
  decisions;
- MCP servers;
- personal profiles or private per-developer memory banks.

Same-repository Git sharing is part of the first release. Memory-only changes
land through existing repository rules and refresh into working branches at turn
boundaries without importing code changes. The selected interface is a portable
skill plus CLI, similar to AWA. v0.0.4 hooks will invoke its shared capture/refresh
operation automatically at turn boundaries even if an agent forgets to do so.
v0.0.2 focuses on daily reliability/recovery and
recall quality as the bank grows, with additional scope informed by v0.0.1 use.
Cross-project federation remains deferred for possible v0.0.4 pending operator
and Vince input, including the unresolved public/private repository boundary.
Optional projections must justify their cost without replacing the files.

## Design litmus tests

A proposed feature fails the mission if any answer is “no”:

1. Can a person understand and repair the current project memory with a text
   editor?
2. Can Codex, Claude Code, and OpenCode use the same bank without semantic
   forks?
3. Does basic checkpoint/resume still work after deleting every generated index
   and disabling every hook or server?
4. Is it clear which source is stronger when memory disagrees with the project?
5. Can stale or poisoned information be corrected without erasing auditability
   or silently reappearing?
6. Is the operational complexity proportionate to an observed need?
7. Can we prove the feature helps an executable task or materially improves
   reliability?
