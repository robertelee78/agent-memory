---
id: ADR-0014
status: proposed
date: 2026-10-24
amends: [ADR-0003, ADR-0005, ADR-0006, ADR-0007, ADR-0009, ADR-0010, ADR-0012]
related: [ADR-0013]
---

# ADR-0014: Useful shared memory and lifecycle assurance

## Context and decision boundary

The [mission review and RCA](../research/2026-10-24-memory-bank-mission-review.md)
found valid repository-bound banks but weak initialization routes, conflated
observation/applicability, and unproven portable checkpoint behavior. It also
identified first-use diagnostic confusion, a bytecode drift mechanism whose
originating process remains unknown, and an existing-directory permission gap.
Lifecycle integrity and recovery controls do not provide release authenticity.

This forward amendment proposes corrective scope for `v0.0.3`, alongside the
reliability, recall and migration work preserved by ADR-0013. It does not rewrite
historical release evidence or imply that every bank is faulty. Proposed status
does not approve execution. Existing work #99 owns extended daily-use evaluation
and #98 owns growing-bank recall; their outcomes remain distinct from the fixes
below. Each requirement below has one corresponding story, directly under this
ADR's root epic. The shared validation requirements apply to all seven stories.

## Distinct initialization routes

Initialization MUST provide a distinct useful purpose and a selective read trigger
for each context file it creates, and MUST preserve already useful existing routes.
Validation MUST exercise different information needs and verify relevant bounded
context choices, rather than accept identical generic startup instructions.

## Observation evidence and applicability

Implementation memory MUST distinguish immutable observation provenance from
semantically reviewed applicability. It MUST preserve evidence and genuine
branch, task or environment conditions without treating a bootstrap branch or
memory-only HEAD change as proof that a lesson is invalid or universally true.
Validation MUST cover unchanged referenced code across a memory-only commit,
changed evidence, and a genuinely branch-dependent claim; unsupported claims MUST
remain qualified rather than automatically broadened.

## Portable scoped handoff

Checkpoint, recall, resume and assembly MUST preserve task/branch conditions and
distinguish inspectable historical orientation from currently applicable guidance.
They MUST NOT turn an unrelated session's next step into the detecting session's
assignment, or imply that machine-local unpublished evidence is remotely available.
Validation MUST cover ordinary recall and explicit historical file inspection
across different tasks and separate clones, including unavailable local artifacts;
it MUST preserve useful evidence and explain qualification or exclusion.

## Actionable first-use diagnostics

Status and doctor MUST distinguish an uninitialized repository from a mismatched
binding, a missing previously known bank and damaged runtime state, and MUST give
the appropriate actionable next step without silently creating project identity.
Validation MUST exercise the public human and JSON commands in each state and
verify that absence is not represented as established corruption or successful setup.

## Safe read-only inspection

Supported inspection guidance and helpers MUST avoid writing Python bytecode into
the installed runtime. Validation MUST show that repeated read-only audit operations
leave its artifact inventory unchanged. Unexpected executable caches MUST remain
visible to integrity checks; recovery MUST preserve evidence and owned source bytes
rather than silently ignore or remove unknown drift.

## Local lifecycle trust paths

Lifecycle operations MUST establish a documented ownership and permission policy
for runtime code, external receipts/journals, locks and relevant ancestors before
trusting those inputs or activating changes. They MUST refuse layouts writable by
untrusted principals and MUST NOT silently chmod user paths. Any intentionally
shared runtime allowance MUST identify its trusted principals explicitly rather
than treat a warning as authorization. This policy MUST remain distinct from
shared repository-bank permissions.
Validation MUST cover private defaults, group/other-writable trust paths, foreign
ownership where testable, changed symlinks and legitimate operator-managed layouts
with clear refusal or explicit trusted-principal qualification and preserved
unrelated data.

## Visible lifecycle security guarantees

Public lifecycle documentation and diagnostics MUST distinguish publisher
authentication from integrity, ownership, compatibility and recovery. They MUST
describe the unsigned mutable Git channel and custom-source trust boundary without
presenting a digest or verified receipt as a signature. Validation MUST exercise
public commands against modified targets, wrong channels, non-fast-forward or
downgrade candidates and interrupted operations, proving refusal/preservation or
bounded recovery rather than merely finding guard code in source.

Signed immutable distribution remains existing story #49 targeted at v0.0.4 until
an explicit operator decision changes that target. This ADR MUST NOT duplicate #49
or imply release signing exists before it is implemented and independently verified.

## Shared validation and exclusions

Each story MUST have source-bound local user-outcome evidence on supported systems,
using isolated repositories/client roots and preserving real installations. The
existing #99 and #98 programs MUST consume these scenarios without duplicating
their broader reliability or scale work. A mechanically valid receipt alone MUST
NOT be presented as proof that shared learning is useful or current.

This scope MUST NOT introduce personal memory, cross-repository transfer, MCP,
new harness hooks, automatic source-policy changes or a new storage engine.
Existing memory MUST be compared before correction; distinct conditions, evidence
and provenance MUST survive deduplication or pruning. Contradictions MUST reach
the detecting developer. No new expensive hosted qualification matrix is required.

Whether the root development bank must be absent from a consumer's entire managed
source checkout remains unresolved under #163. These stories MUST NOT silently
choose that distribution policy or claim the pending adoption has landed.
