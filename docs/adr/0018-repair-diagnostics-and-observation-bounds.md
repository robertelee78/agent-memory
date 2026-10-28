---
id: ADR-0018
status: accepted
approval-scope: capacity-diagnosis-for-v0.0.10
amended: 2026-10-28
date: 2026-10-26
related: [ADR-0007, ADR-0009, ADR-0012, ADR-0014, ADR-0015, ADR-0016]
---

# ADR-0018: Repair diagnostics and bounded observation

Originally proposed for **v0.0.4** from independent static review of v0.0.3.
The operator selected the capacity-diagnosis correction for **v0.0.10** on
2026-10-28, bounded by public reproduction and independent design review.
Acceptance is limited to that outcome; it does not retroactively approve other
sections or alter delivered releases and their separate acceptance evidence.
The implementation attempt requires an actual independent approval of this
immutable design, not this status field alone.
The [review RCA and triage](../research/2026-10-26-static-review-followups.md)
separates source-confirmed mechanisms from pending executable reproductions.

## Context

The operator expects quiet routine operation and useful recovery instructions
when something fails. Existing ownership, source freshness, private dispute
history and bank bounds must survive recovery. Static reviewers found safe
refusals whose diagnosis or repair can improve, plus an unbounded observation
path before format validation. These are follow-ups to existing lifecycle and
sharing behavior, rather than a new installation or memory architecture.

## Outcomes and approval scope

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

A fully structurally validated successor to the verified current dispute registry
SHALL distinguish capacity exhaustion from malformed or interrupted persisted
state. The public resolution-history reproduction accepted 4,095 to 4,096
predecessor entries, then refused 4,096 to 4,097 with a generic review diagnosis
while preserving original state. This confirms a diagnostic defect, not data loss.

The unchanged limits are 4,096 dispute records, 4,096 predecessor resolution
entries **per record** (the current resolution is separate), and 16,777,216
encoded bytes for the complete sealed registry. The exact boundary SHALL remain
supported. A proposed successor exceeding one SHALL fail before replacing any
journal, registry, recovery copy or anchor; canonical memory, feature work, index,
HEAD, pending review and every retained predecessor SHALL remain intact.

Proposed-state validation MAY separate structural validity from capacity solely
for the successor derived from verified current state and validated review. It
MUST validate the full schema/checksum and every record/history entry before
classifying excess; malformed proposed data MUST NOT be certified as capacity.
Persisted-state readers SHALL retain their existing validation and recovery
diagnoses; an already oversized, malformed, checksum-invalid or incomplete
persisted store MUST NOT become trusted through this path.

Typed HoldFailure outcomes SHALL use `known-holds-record-capacity`,
`known-holds-history-capacity`, or `known-holds-byte-capacity`, carried through
existing semantic/publication/refresh error surfaces as `known-dispute-state`.
The reason SHALL identify the measured amount, actual limit and exhausted
dimension. When several limits are exceeded, primary classification SHALL use
fixed precedence: record count, then per-record history count, then byte size.
Byte size SHALL be measured from the same complete sealed encoding used by the
persisted-state guard, not an unsealed payload or a journal envelope. Repair guidance SHALL preserve local learning and all evidence, state
that the current runtime cannot retain the proposed additional metadata, and
require a supported reviewed capacity/retention change. Replaying recovery or
retrying the same write does not increase capacity. This correction MUST NOT
raise limits, prune history, remove disputes, introduce automatic archival, or
mark unfinished review/publication as completed.

Public-command acceptance SHALL exercise each actual boundary and excess using
valid fixture state derived from a real public review; fixtures need not perform
thousands of redundant CLI renewals. It SHALL preserve old failing results,
validate exact unchanged-state hashes after refusal, and test both retry cases.
Reapplying a proof already recorded at an exact limit SHALL succeed as a no-op
with byte-identical state and no capacity error. Retrying a refused excess SHALL
return the same typed capacity refusal and byte-identical state without marking
anything completed. A combined-excess case SHALL verify the fixed precedence. Malformed/checksum-invalid and interrupted persisted state,
and malformed proposed input, SHALL remain negative controls with their original
non-capacity diagnoses. Focused qualification SHALL run on macOS and Linux within
the operator's 10–15-minute per-test budget. Independent review SHALL bind actual
implementation and evidence before this outcome becomes release-ready.

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
Capacity-diagnosis delivery SHALL be an independently accepted implementation
in the published v0.0.10 release; other outcomes retain their recorded delivery
history rather than inheriting this new approval. For every outcome, a static review or planning document is not delivery evidence.

## Consequences

Failures can become easier to diagnose without weakening ownership, currentness
or evidence preservation. The capacity response and safe retention options need
explicit design; an arbitrary automatic deletion policy would conflict with
retained dispute provenance. Fresh-install signed-release freshness still rests
on the documented HTTPS/GitHub bootstrap/locator trust boundary; this proposal
does not add an expiry service or promise independent freshness authentication.
