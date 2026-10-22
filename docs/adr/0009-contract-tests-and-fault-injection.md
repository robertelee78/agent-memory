---
id: ADR-0009
status: proposed
date: 2026-10-21
depends-on: [ADR-0003, ADR-0006, ADR-0007]
---

# ADR-0009: End-to-end contract tests and incident-derived regressions

## Context

A memory system can pass unit tests while losing the exact property users care
about: a fresh client reliably resumes from durable, correct, human-readable
state. Installation, atomic file replacement, interrupted checkpoints, client
activation, stale indexes, deletion, and concurrent sessions cross component
boundaries.

AWA's strongest tests invoke the real CLI in isolated temporary environments,
model realistic remote behavior, inject failures after writes but before
responses, and preserve real incidents as named regressions. Project 24 ties
those regressions to outcomes, acceptance evidence, integration, and releases.

## Decision

The repository SHALL provide one top-level test gate that runs these layers:

1. portable-pack and progressive-route validation;
2. install, update, uninstall, rollback, and recovery tests;
3. Memory Bank initialization, resume, checkpoint, correction, revocation, and
   deletion contract tests;
4. concurrency and interruption tests against real temporary filesystems;
5. Codex, Claude Code, OpenCode, and generic Agent Skills adapter tests;
6. optional index/federation rebuild and isolation tests;
7. matched memory-on/off outcome and security evaluations where applicable.

End-to-end tests SHALL invoke the same public CLI and scripts used by people and
agents. They MUST NOT substitute direct calls to internal helpers for acceptance
proof. Unit tests MAY cover algorithms and parsers but SHALL NOT be the only
evidence for a user-visible contract.

Each test world SHALL use isolated temporary home, XDG, client configuration,
repository, bank, runtime-state, cache, and backup roots. The default suite MUST
NOT require real credentials, alter live client configuration, touch a
developer's Memory Banks, or depend on network availability.

The fault harness SHALL be able to interrupt before and after every durable
write and to simulate a write that commits successfully before its response is
lost. Rerunning the same operation MUST converge without duplicating, losing,
or silently weakening memory.

The suite SHALL include true concurrent processes operating on the same bank.
It SHALL verify observed-hash conflicts, lock recovery, atomic replacement,
bounded retry, and the absence of last-write-wins data loss.

The first release SHALL additionally exercise independent Git clones, separate
worktrees and branches, two developer identities, and multiple harness/session
identities against an isolated shared remote. A lesson published from one clone
SHALL be recalled by another at its next successful turn-boundary refresh while
non-memory working files, staged changes, and feature history remain unchanged.
The suite SHALL test simultaneous publication, three-way reconciliation, retry
after a successful push or merge with a lost response, offline retry, and
replay after a later ordinary code merge without duplicated learning.

Fixtures SHALL cover same-concept deduplication before write and before landing;
compatible edits to the same file; semantic contradictions that Git merges
cleanly; yellow/blue decisions for the same wall; conditional lessons about
unmerged code; stale code receiving newer memory; and evidence-based pruning
that preserves still-applicable older-branch guidance. Clarification SHALL go
to the detecting developer, cross-developer provenance SHALL be visible, and
only disputed memory and dependent work SHALL pause. Initialization fixtures
SHALL combine code/tests, docs/ADRs, and AWA work evidence, preserve existing
memory, and distinguish genuine contradictions from different lifecycle states.

For `v0.0.2`, reliability and recovery in daily use SHALL be a release focus
alongside growing-bank recall in ADR-0005. Extended user-level scenarios SHALL
exercise repeated multi-session/multi-machine use, offline intervals, competing
writers, interruption, recovery, and later ordinary code integration. They SHALL
assess usable memory after recovery, pending/freshness reporting, repeated
failures, and the clarity and effort of recovery. Representative workload sizes
and measurable acceptance targets SHALL be refined from operator input and
v0.0.1 experience before implementation claims. Baseline data integrity and safe
sharing remain v0.0.1 requirements; this focus does not defer those guarantees.
Additional post-v0.0.1 findings MAY expand the v0.0.2 plan.

For optional stores and indexes, tests SHALL prove that:

- deleting all generated state leaves the Markdown bank usable;
- rebuilding reproduces current visible memory;
- revoked or deleted content does not reappear;
- project and federation scopes do not leak; developer attribution stays accurate;
- an unavailable optional service does not block local checkpoint/resume.

Every production failure in scope SHOULD first receive the smallest user-level
reproduction that fails on the old behavior. The regression name SHOULD retain
the incident's user-visible symptom. The corresponding work item SHALL record
the outcome, executable acceptance, integration evidence, and delivery boundary.

Quoted commands, result fields, errors, and exit meanings in skill guides SHALL
be captured from observed test or read-only production output. A documentation
refactor SHALL include a rule-by-rule map proving that every normative
instruction survives at equal or stronger RFC 2119 force and remains reachable
through the progressive route graph. Independent agent reviews SHOULD exercise
representative leaves using only the instructions that would load at runtime.

## Consequences

- Tests specify the product across file, process, and client boundaries.
- Failure recovery and data survival become release gates rather than informal
  expectations.
- The suite is larger and slower than parser-focused unit tests.
- Optional adapters require maintained contract fixtures and occasional
  isolated live calibration.

## Alternatives considered

- **Unit tests only:** rejected because they cannot prove lifecycle, process,
  filesystem, and client integration contracts.
- **Always test against live clients and services:** rejected because it is
  nondeterministic, credentialed, and unsafe as the default gate.
- **Treat incidents as documentation only:** rejected because the same failure
  can recur without an executable regression.
