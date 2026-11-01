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

## Proposed amendment: Consistent bank-source eligibility

**Proposal dated 2026-10-31; not covered by the accepted v0.0.10 capacity scope.**
Stable proposed work key: `ADR-0018:bank-validation-consistency`. This outcome is
not selected for the v0.0.13 installer release. It requires independent design
approval and separate implementation/acceptance/delivery evidence.

The [secondary RCA](../research/2026-10-31-bank-size-and-owned-hook-health-rca.md)
reproduced a native context that `am bank validate` calls valid while ordinary
inspection, scoped recall and capture refuse the shared source-size bound.
Format validity and source availability are different facts, but public file-
contract success MUST NOT conceal a source limit required by every normal reader.

The proposed common bank eligibility check SHALL enforce the existing complete
inventory, path/type/alias, interruption and resource constraints before any
normal operation reports the bank usable. The per-source upper bound SHALL
remain **16,384 UTF-8 bytes for the complete canonical file**, including metadata;
it is not only a router or record-body limit. Existing lower capture, record,
request and disclosure bounds SHALL remain unchanged. Aggregate/file-count,
router/index and schema limits SHALL NOT be increased by this amendment.

`bank validate`, doctor bank checks, inspect, discovery, recall and mediated
writer preconditions SHALL use that same eligibility definition. A renderer or
writer preview that returns intended canonical files SHALL apply their actual
size/layout constraints before describing the result as valid for normal use.
Format-only subresults MAY remain separately labelled, but MUST NOT make the
outer operational eligibility result successful while required source checks
failed. These checks MUST be bounded while reading; validation MUST NOT first
load an arbitrarily large source into memory.

Source, aggregate-byte and canonical-file-count refusals SHALL respectively
preserve typed `bank-source-byte-capacity`, `bank-aggregate-byte-capacity` and
`bank-source-count-capacity` causes
through the public operation's error envelope, with a fixed phase, canonical
relative path, bound and truthful size measurement. A stable descriptor size MAY
supply an exact byte count without reading the oversized body; otherwise the
result SHALL state the measured lower bound or changed/unavailable observation.
Unsafe paths, redirected/special files, active transaction evidence and changing
sources MUST retain their existing distinct refusals. A capacity result MUST NOT
assert that the source would otherwise be structurally or semantically valid.
Diagnostics MUST NOT disclose memory bodies as part of the failure.

An unrelated `--path`, ID or lexical selector MUST NOT skip an unreadable source
when completeness is required for recall or pre-write comparison. A refusal
SHALL remain **unavailable/pending**, never a successful empty bank, novelty
proof, inferred missing terminology or fabricated no-learning assessment. Task
answers MAY continue independently with that qualification; new governed capture
MUST remain pending. Repeating the same operation does not repair capacity.

Read-only diagnosis SHALL preserve canonical files, original index/HEAD, feature
work and recovery evidence. Guidance SHALL point to separately reviewed
source-preserving layout recovery under ADR-0010 only when actually available.
Until then it SHALL state that recovery is unavailable, preserve the bank and
report capture pending instead of naming a nonexistent command. It MUST NOT advise
truncation, deletion, an ignored oversized source, a raised normal cap, or
initialization over an existing bank.

Acceptance SHALL retain the old contradictory public results and exercise an
otherwise valid native context at 16,384 and 16,385 bytes, a larger synthetic
reported-size example, aggregate/count bounds and changed/unsafe inputs. Normal
validation, full and scoped inspection/recall, and a small unchanged capture
candidate SHALL agree on eligibility while retaining their separate semantic
and mutation guarantees. Unsupported layout MUST retain its own diagnosis when
size is within bounds. A supported validator MUST NOT claim a terminology rule
was admitted merely because the bank is readable. Tests SHALL use disposable
banks on macOS and Linux within the existing per-test time budget.

## Consequences

Failures can become easier to diagnose without weakening ownership, currentness
or evidence preservation. The capacity response and safe retention options need
explicit design; an arbitrary automatic deletion policy would conflict with
retained dispute provenance. Fresh-install signed-release freshness still rests
on the documented HTTPS/GitHub bootstrap/locator trust boundary; this proposal
does not add an expiry service or promise independent freshness authentication.
