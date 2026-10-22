---
id: ADR-0006
status: proposed
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002, ADR-0005]
---

# ADR-0006: Security boundaries and executable evaluation

## Context

Persistent memory extends the lifetime of prompt injection beyond one context
window. A poisoned record may influence many future sessions and clients.
Recall-only benchmarks do not establish that memory helps repository work, and
successful task completion does not prove that a newly formed memory is safe or
causally useful.

## Decision

Memory SHALL be treated as untrusted data when retrieved. Only separately
reviewed procedural memory may become instructions, and its source, version,
scope, and approval SHALL remain visible.

CLI, MCP, hook, and adapter operations SHALL enforce:

- separation of user statements, assistant claims, tool observations, external
  content, and verified repository evidence;
- per-principal and per-project authorization before retrieval;
- explicit approval for global promotion and procedural promotion;
- redaction and secret scanning before persistence;
- size and content limits on capture and context assembly;
- quarantine for suspected instructions or unverifiable identity claims;
- durable deletion tombstones and cache/index invalidation;
- audit events for every mediated admission, mutation, retrieval, and promotion;
- no shell interpolation of retrieved memory or external content;
- no execution of commands merely because a memory contains them.

Direct filesystem access is intentionally outside that mediation boundary. A
person or process with file access can read or edit the Markdown bank without a
retrieval audit. Confidentiality for direct access is provided by operating-
system permissions, repository access, and Git hosting policy. The documentation
MUST NOT claim that `agent-memory` can authorize or audit a raw editor, `cat`, or
Git checkout. A later mediated shared service MAY add stronger controls without
changing the local bank format.

In v1, per-principal authorization is bounded by the operating-system account
and filesystem permissions. Principal IDs prevent accidental logical mixing but
do not authenticate mutually distrusting users who share one OS account.
Security tests and product claims SHALL preserve that distinction.

Automatic project-memory capture and publication SHALL apply the same source,
secret-scanning, scope, and quarantine rules before committing shared files.
Permission to maintain shared project memory MUST NOT imply permission to share
private preferences, raw conversations, or unreviewed procedures. The first
release SHALL enforce these baseline controls on its automatic path.

Memory MAY inform a task, but it MUST NOT override the current operator request,
the portable skill's safety and authority rules, repository instructions, or a
stronger authoritative artifact. Only an explicitly reviewed and promoted
procedure MAY become procedural guidance, and even then it remains subordinate
to those authorities. Commands in ordinary project context are examples or
facts until the current task independently chooses to use them.

The first release SHALL include structural, lifecycle, scope-isolation, deletion,
and poisoning canaries. Before the project claims that memory improves coding
outcomes, it SHALL also provide a matched memory-on and memory-off evaluation
harness. The complete evaluation program SHALL measure:

- task resolution by executable tests;
- repeated-failure avoidance;
- agent steps, tokens, latency, and storage growth;
- admission precision and recall;
- retrieval recall, ranking quality, and abstention;
- correct knowledge updates and selective forgetting;
- cross-project and cross-principal leakage;
- poisoning at content, evidence, scope, selection, and consolidation-path
  interfaces;
- reproducibility across Codex, Claude Code, and OpenCode adapters.

A memory-assisted result SHALL NOT be called an improvement unless its matched
baseline, memory contents, retrieved context, and executable outcome are
recorded. A promoted reusable procedure SHOULD be tested on held-out
repositories or tasks and SHOULD include an applicability predicate.

The project SHALL maintain adversarial fixtures for stale facts, malicious
repository documents, poisoned tool output, conflicting user preferences,
deleted data, ambiguous repository identities, and hostile memory files.

## Consequences

- Automatic capture cannot imply automatic trust.
- The evaluation harness is a product component, not post-hoc research work.
- Some convenient cross-project behavior will require explicit user action.
- Security and quality regressions can be localized to memory-formation or
  memory-use stages.

## Alternatives considered

- **Rely on model judgment to ignore malicious memory:** rejected by empirical
  poisoning results.
- **Evaluate with question answering only:** rejected because it does not prove
  useful software changes.
- **Trust memories written after successful tasks:** rejected because success
  does not establish which behavior caused the outcome, and memory formation
  itself can be manipulated.
