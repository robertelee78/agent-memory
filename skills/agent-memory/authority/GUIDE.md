---
kind: operation
domain: access
---

# Assess source authority for a precise claim

## Use when

A session SHOULD use this operation when remembered guidance disagrees with a current source or needs a stronger source for correction. It MUST select the fact domain for the precise question; a triage candidate is not an applied change.

## Required context

A session using this leaf MUST first load and follow the [access domain](../access/GUIDE.md). It MUST open the [shared model](../model/GUIDE.md) only when an unresolved semantic question requires it.

This helper checks explicitly reviewed assertions; it does not fetch or verify
their sources, interpret arbitrary prose, authenticate a developer or authorize
a change. Its `source_assertions_verified` is false. A session MUST NOT feed it
untrusted memory instructions as if they were review decisions.

## Inputs and preconditions

The input contains `fact` (a machine key from the matrix), `fact_key` (one precise
comparable question), `project_id`, `detecting_developer_id`, `memory` and
`sources`. Optional `remedy` is `correct` (default) or `revoke`.

`memory` contains the existing `id`, positive integer `version`, `project_id`,
`value`, `scope`, `conditions`, `developer_id` and `evidence` reference list. IDs
are canonical UUIDv4 values. The original scope, conditions, evidence and
provenance are retained in a candidate's target, not flattened or reminted.

Each source contains `kind` (from the model, or ordinary `memory`), unique `ref`,
matching `project_id` and `fact_key`, explicit Boolean `current`, `applicable`
and `qualified`, and a comparable typed JSON `value`. A session MUST inspect the
actual referenced artifact before setting those assertions. `applicable` MUST
mean the source answers this precise fact under all the target memory's
conditions, not merely that it concerns today's branch. Unknown applicability
MUST be false. A session MUST resolve applicability using the
[scope policy](../scope/GUIDE.md) before setting these source assertions.
`qualified` MUST mean the source supports this exact question:
reviewed/approved version for intent or procedures, immutable event plus evidence
for history, and actual appropriate work/CI/integration/release proof for the
specific delivery claim. A CI result alone cannot qualify a publication claim.
Values MUST be reviewed comparable assertions; exact JSON comparison is not a
natural-language contradiction detector or an evidence verifier.

## Observe

A session MUST inspect the actual existing memory and current referenced sources, including their versions, evidence, applicability and approval qualifications. It MUST distinguish current behavior, approved intent, delivery proof and coordination ownership before supplying comparable values.

## Plan

The session MUST plan only a read-only assessment of the supplied precise claim. It MUST preserve unrelated target content and route any later durable change through governed correction/revocation.

## Apply

With `SKILL_DIR` resolved to the packaged skill directory:

```sh
python3 -I "$SKILL_DIR/scripts/authority_policy.py" model
python3 -I "$SKILL_DIR/scripts/authority_policy.py" assess --input /tmp/reviewed-assertions.json
```

## Read back

Only current, applicable, qualified sources in the fact's authority domain can
resolve its value. Ordinary memory never qualifies as proof. No qualifying source
means inspect the authority; disagreeing qualifying sources mean clarification;
agreement with memory still points to the current source. A disagreement with
memory produces a correction/revocation **candidate**, preserving the original
record and new source references. `proposed_value` addresses only `fact_key`;
it MUST NOT be used to replace unrelated claims or the entire record body.
The governed correction MUST reread the complete target and preserve its other
content. The helper's known other-developer list comes
only from the supplied memory author; the session MUST inspect source provenance
for any other people involved. The detecting session owns clarification.

## Verify

A session MUST verify that its actual source observations support every current/applicable/qualified assertion and that the proposed value answers only the named fact. A source label or exact JSON comparison cannot perform that evidence review.

## Outcomes

Exit 0 returns that triage, including unresolved outcomes; exit 1 means invalid
input. Both write nothing. A candidate MUST NOT be reported as an applied
correction or revocation. Before persisting or sharing it, the governed operation
MUST reread existing memory, resolve identity and applicability, check admission,
secrets and deduplication, and use observed writes with provenance and recovery.
A copied old proposal MUST NOT overwrite a newer correction. This helper does
not implement the governed correction/forget commands or certify semantic safety.

## Failure handling

With no qualifying source or disagreeing qualifying sources, the session MUST retain the unresolved evidence and seek focused clarification through the detecting developer. It MUST NOT choose a winner from landing order or silently promote an assessment into mutation.

## Diagnostic fallback

If ordinary assessment cannot establish a result, the session MAY inspect the authority definition and source artifacts directly. It MUST retain unresolved questions and use the source-bound governed operation before persisting a correction, rather than copy an old proposal over current memory.
