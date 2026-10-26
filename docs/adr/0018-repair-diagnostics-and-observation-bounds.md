---
id: ADR-0018
status: proposed
date: 2026-10-26
related: [ADR-0007, ADR-0009, ADR-0012, ADR-0014, ADR-0015, ADR-0016]
---

# ADR-0018: Repair diagnostics and bounded observation

Proposed for **v0.0.4**, based on independent static review of the v0.0.3
candidate. This proposal does not authorize implementation, change v0.0.3's
acceptance criteria, or claim a reproduced production failure for these paths.
The [review RCA and triage](../research/2026-10-26-static-review-followups.md)
separates source-confirmed mechanisms from pending executable reproductions.

## Context

The operator expects quiet routine operation and useful recovery instructions
when something fails. Existing ownership, source freshness, private dispute
history and bank bounds must survive recovery. Static reviewers found safe
refusals whose diagnosis or repair can improve, plus an unbounded observation
path before format validation. These are follow-ups to existing lifecycle and
sharing behavior, rather than a new installation or memory architecture.

## Proposed outcomes

### Typed publication recovery

Publication SHALL preserve a recognized private-dispute recovery failure's
specific code, component and repair guidance instead of presenting it as an
ordinary source-reconciliation conflict. It SHALL preserve the pending request,
bank, feature edits and source evidence. Recovery SHALL NOT imply that a stale
proof is current or that an unresolved contradiction has been clarified.
Acceptance SHALL reproduce missing/interrupted dispute state through public
publication commands, verify the actionable diagnosis and supported recovery,
and retain ordinary stale-source refusal and successful publication controls.

### Capacity diagnosis

A recognized dispute-registry/history capacity limit SHALL be reported as
capacity exhaustion, separately from corruption or incomplete recovery. The
message SHALL identify the actual limit and a truthful next step; it MUST NOT
suggest that replaying recovery will increase the supported capacity. It MUST
NOT discard a known dispute or its evidence to force a successful renewal.
Acceptance SHALL exercise the supported boundary and an excess through public
commands, verify refusal before durable replacement, and preserve the original
registry, recovery evidence, bank and feature state. Retention/archival policy
remains a design question; this proposal does not authorize destructive pruning.

### Verified legacy uninstall repair

Uninstall SHALL distinguish a verified metadata-only change in an owned legacy
managed Git runtime from an unknown payload or ownership change. It SHALL reuse
the existing recorded-revision/payload verification boundary before treating that
source as removable. Modified/untracked payload, unrelated paths, live writers
and sources still referenced by preserved client targets SHALL remain protected.
Acceptance SHALL reproduce metadata-only drift through the public lifecycle,
verify the intended uninstall outcome, and retain negative ownership,
modified-payload, preserved-client and interrupted-operation controls.

### Bounded prevalidation observation

Bank hashing before validation SHALL enforce the existing observation resource
and path/type bounds while reading, rather than reading an arbitrarily large
Markdown source into memory before checking those bounds. It SHALL retain
complete canonical hashes and changed-source detection for supported banks.
Oversized or unsafe input SHALL fail with a useful bounded outcome, preserve
canonical memory and feature state, and MUST NOT become a successful empty bank.
Acceptance SHALL cover a single oversized source, aggregate excess, symlink/type
refusal, supported near-boundary banks and source changes during observation
through actual public snapshot/diagnostic/sharing operations.

## Shared acceptance and boundaries

Each outcome SHALL first obtain a minimal public-operation reproduction and an
independently approved design. Validation SHALL retain source-bound Mac/Linux
results and exact preserved state. Work SHALL use the existing portable skill,
CLI, ownership/recovery mechanisms and same-repository Markdown bank. It MUST
NOT introduce personal memory, cross-repository sharing, MCP or a required
service. Tasks SHALL remain Backlog until their designs are actually approved.
Delivery SHALL be an independently accepted implementation in the published
v0.0.4 release; a static review or planning document is not delivery evidence.

## Consequences

Failures can become easier to diagnose without weakening ownership, currentness
or evidence preservation. The capacity response and safe retention options need
explicit design; an arbitrary automatic deletion policy would conflict with
retained dispute provenance. Fresh-install signed-release freshness still rests
on the documented HTTPS/GitHub bootstrap/locator trust boundary; this proposal
does not add an expiry service or promise independent freshness authentication.
