---
id: ADR-0017
status: accepted
approval-scope: architecture-only; execution design gated on ADR-0024 G1
date: 2026-10-25
updated: 2026-11-04
depends-on: [ADR-0004, ADR-0005, ADR-0014, ADR-0023, ADR-0024]
---

# ADR-0017: Standalone semantic terminal recall in v0.0.16

## Why and decision scope

The operator deferred plain-terminal semantic recall beyond v0.0.3 and selected
v0.0.16 for existing story #200. Literal `am search` is not semantic
interpretation. A terminal user needs the same qualified saved guidance and
optional relationship navigation available to an agent, without an already
active coding-agent session.

The 4 November PM decisions select saved guidance, not a newly synthesized
answer, and explicitly include an embedding model, graph database, and SSG
runtime adaptation. [ADR-0023](0023-guided-progressive-recall.md) defines the
experience and [ADR-0024](0024-ssg-graph-embedding-runtime.md) defines its runtime.
Independent [Opus review](../research/2026-11-04-ssg-am-architecture-review.md)
accepted this architecture, not implementation or qualification evidence.
Task-specific execution design remains gated on G1. Uppercase terms use
[RFC 2119](https://www.rfc-editor.org/info/rfc2119/).

## What: shared terminal/core contract

The public CLI MUST offer explicit semantic selection through
`am search --semantic QUERY`, while existing literal/exact search MUST remain
unchanged when semantic selection is absent. The semantic operation MUST use
ADR-0024's reviewed graph profile and the operator-configured compatible
embedding endpoint/model (local by default, approved remote optional). It MUST
NOT require an active agent or second answer-generating model, and MUST return
applicable saved guidance with source project, record/version, conditions,
evidence, and availability.

Related-memory navigation MUST be separately callable and bounded. Responses
MUST provide a stable, source/generation-bound selection reference for an
explicit `am related REFERENCE` request; stale references MUST be requalified or
refused, not trusted as saved authorization. Human and machine output MUST
distinguish recorded relationships from inferred suggestions. Mandatory
answer-changing caveats MUST accompany primary guidance, not depend on a
follow-up command.

The operation MUST default to the current project. Cross-project expansion MUST
use ADR-0023's explicit authorized request/approval gate and MUST NOT infer export
permission from repository access. Approval MUST come through ADR-0023's trusted
operator-confirmation channel; a terminal or pseudo-terminal alone MUST NOT
prove consent. Callers without a valid operator grant MUST receive a structured
approval-required outcome and perform no foreign query. No MCP or
personal-memory namespace is introduced.

The terminal command MUST use the shared native reader and
post-qualification/pre-budget ranking seam. It MUST preserve project, scope,
trust, validity, visibility, applicability, supersession, pending/held-source,
provenance, and final source/policy checks. Discovery completeness, current
source qualification, semantic availability, and remote freshness MUST remain
distinct states. Optional stores remain required delivered features but are not
prerequisites for baseline local memory continuity.

Unavailable interpretation MUST return an actionable non-success semantic
outcome. An explicitly requested literal fallback MAY return literal results,
but MUST identify that mode and MUST NOT report a semantic no-match. Setup,
model download/start, endpoint disclosure approval, diagnostics, and cleanup
MUST follow ADR-0024; search MUST NOT silently download a model, start a service,
or transmit repository/query content to an unapproved endpoint.

## Acceptance Criteria

- Actual installed terminal commands MUST retrieve independently labeled
  applicable paraphrases, including examples with no required literal overlap,
  without an active coding-agent session.
- Saved guidance, mandatory caveats, stable related-memory references, and
  recorded/inferred relation labels MUST be observable in human and machine
  output; optional bodies MUST appear only after explicit follow-up.
- Missing conditions, stale/corrected/deleted guidance, unsafe sources, hidden
  replacements, disputed/held paths, and unresolved contradictions MUST retain
  native qualification/clarification boundaries.
- Unavailable services, refused disclosure, malformed model responses, stale
  references, and expansion without a verified operator grant MUST produce
  explicit actionable outcomes; literal fallback MUST NOT claim semantic
  success or completeness.
- Evaluation MUST retain source/runtime/model identity, quality, native/model
  latency, context disclosure, and cost separately at the ADR-0024 agreed sizes.
  Numeric targets MUST be frozen before a performance pass is claimed.
  Quality/resource passes MUST apply only to the measured embedding profiles;
  other configured profiles MUST report their own compatibility/status rather
  than inherit a reference-model pass.
- Existing #200 MUST ship in signed v0.0.16 with public command/setup/privacy/
  troubleshooting documentation and independent actual-terminal acceptance.
  A harness-only demonstration MUST NOT satisfy this outcome.

## Dependencies and exclusions

ADR-0024 G1's reviewed deployment/execution contract, G3 local graph runtime,
G2 embedding/ranking, and P1's shared disclosure contract supply the execution
prerequisites. G3/G2/P1's independent local destructive/fault qualification
supplies the local release gate. Cross-project execution additionally requires
#55/#57 and ADR-0023 P2; within-project terminal recall MUST NOT wait for #56,
#62, P2, or federation. No local-only evidence completes the separately retained
cross-project stories.

The exact machine response/exit-code schemas, supported dependency profiles, and
measured tolerances MUST be fixed in G1 before dependent implementation is Ready.
This ADR selects the command/experience direction, not unmeasured compatibility
or performance claims.
