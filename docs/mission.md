# Mission and product doctrine

## Mission

Take what is good about Cline's Memory Bank, package it with the progressive
skill and operational discipline of `agent-work-accountability`, and improve it
without turning it into a brittle memory platform.

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
6. **Maintenance is visible.** The agent does not silently ingest every chat and
   claim that accumulation equals understanding.

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
- Keep project-local memory separate from optional personal preferences and
  explicit cross-project federation.
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
- a promise that hooks can perfectly infer what deserves to be remembered.

MCP, hooks, embeddings, SQLite, synchronization, and graphs are optional later
adapters or projections. None belongs on the critical path until the file-and-
skill workflow is demonstrably insufficient.

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
