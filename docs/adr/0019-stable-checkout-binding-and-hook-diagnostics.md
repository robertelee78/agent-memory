---
id: ADR-0019
status: proposed
date: 2026-10-27
amends: [ADR-0011, ADR-0004, ADR-0018]
---

# ADR-0019: Survive filesystem device changes and explain pending hooks

## Context and research

On 2026-10-27 the installed v0.0.6 runtime rejected three previously bound
repositories on the operator's Mac. projects.json retained checkout keys with
device 16777234; current stat observations reported device 16777230 with the
same respective inodes and canonical locations. agent-memory, ioa-brain and
cve-to-exploit all reported binding_required. Vox contained both keys after an
explicit relink. The Mac booted after the original registrations; the precise
OS event changing the device number is not established.

project_identity.discover constructs its lookup key from st_dev:st_ino. status
requires an exact registry lookup, so a device-number change defeats a prior
binding even when project UUID, location and authority still match. Both Codex
hooks call scope_binding.require before retaining or processing the turn. No
Codex boundary ledger was present for this checkout. A controlled valid-shaped
Stop event reproduced the generic pending message. The original failing event
was not retained, so its first failing check cannot be reconstructed exactly.

The hook catch-all discards failure stage and reason. Existing test_codex_stop
uses a freshly bound fixture and tests malformed inputs; it does not simulate a
device-number change. Registered/trusted hooks and successful process exits do
not establish completed capture/publication. The main checkout is also behind
origin/main; this does not explain the installed runtime's identity failure.

## Decision

### T1: Recover a previously established checkout binding

The implementation MUST preserve project and developer UUIDs and bank bytes.
An exact current binding remains authoritative. When it is absent, resolution
MUST recognize a semantically unique historical binding with the same canonical common-Git
location and inode despite a different device number, only when the registered
project UUID, bank UUID, bank canonical remote and current authority agree.
Equivalent historical keys for the same project and authority count as one
decision; conflicting project or authority decisions MUST be refused. Hooks and
all supported harnesses MUST continue through the shared read-only resolver when
these conditions hold. The effective registry key MUST be deterministic and
retained in operation guards; the observed filesystem key remains separately
visible. Explicit saved authority selections MUST be preserved.
Conflicting candidates, changed inode, changed authority, copied banks, missing
bank identity and independent clones MUST remain unresolved. A remote match
alone MUST NOT adopt a checkout. Linked worktrees MUST use their common Git
directory. Read-only resolution MUST NOT write registries. This compatibility
rule MUST be explicitly reported as recovered historical evidence, rather than
claim a device number is durable identity or authentication.

### T2: Diagnose and repair the binding

am doctor MUST identify a recognized historical binding and distinguish it from
a new checkout. am doctor --fix MUST persist a verified current binding through
the existing preview/apply protocol when T1's strict conditions hold, retaining
history. It MUST recheck evidence and ordinary concurrency guards. Ambiguous
cases MUST include specific next steps without modifying memory or identities.

### T3: Explain hook failures

Codex prompt and stop adapters MUST expose bounded, fixed reason codes and the
failed stage without retaining prompt text, final answers, secrets or arbitrary
exception strings. Failures MUST remain pending and preserve work. am doctor
MUST expose recent same-repository hook failure metadata and actionable repair
instructions where available. A hook process exit, injected instruction or
no-learning acknowledgment MUST NOT be reported as published learning.

### T4: Validate actual user behavior

Tests MUST cover device changes, inode replacement, conflicting matches, remote
changes, independent clones, linked worktrees and repeated repair. Hook tests
MUST cover prompt-to-stop-to-acknowledgment with a recovered binding and report
precise failure reasons. Real harness checks MUST distinguish invocation,
semantic assessment, local admission, published memory and fresh recall.
Claude, Codex and OpenCode use the same identity rules. Local focused tests
SHOULD complete within 10–15 minutes; broad unrelated gates are excluded.

## Alternatives considered

Dropping the device component permits inode collisions across volumes. Path-only
lookup could adopt a replaced clone. Remote-only lookup could adopt an independent
clone. Platform volume UUID APIs add platform-specific dependencies. A random
marker in the common Git directory requires Git writes and copied-marker handling,
including sandbox constraints. We select conservative historical continuity with
existing evidence and no extra Git writes. This is stable project continuity, not
a claim that device numbers are stable or identity is authentication. Repeated
ordinary resolution writes nothing and does not grow the registry.

Failure metadata SHALL use one replaceable owner-only bounded JSON record per
canonical common-Git location, keyed by its SHA-256 under the private runtime
state root, outside repository memory. Records SHALL contain only schema, fixed
harness/stage/reason, and timestamp; at most 4096 bytes. No transcript, prompt,
final answer, arbitrary exception string or credentials are retained. A subsequent
successful hook clears its own failure state; absence never proves publication.
Early failures before safe repository discovery report the fixed code without
writing. Codes include event.invalid, repository.unavailable, identity.unresolved,
boundary.missing, boundary.unsafe, boundary.source_mismatch, and state.unavailable.
Stages are event, repository, identity, boundary and guidance.

Repair MUST use existing registry locks and optimistic read-set revalidation.
It MUST NOT rewrite previously prepared proposals to bypass changed evidence.
A prompt that failed before retaining its source MUST be retried as a real prompt
after repair; no source identity or completion receipt may be fabricated.

## Delivery

This corrective work targets v0.0.7. Independent Opus review precedes acceptance
of this ADR and source-bound task extraction. Release publication requires
focused passing checks, independent implementation review, signed artifacts,
public install/update verification and AWA delivery readback. Unrelated existing
v0.0.7 work MUST NOT be silently represented as delivered.
