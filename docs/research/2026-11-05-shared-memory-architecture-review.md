# Independent Opus review of the shared-memory product amendment

## Reviewed sources and verdict scope

Two independent read-only review agents used `anthropic/claude-opus-5-5:high` on
5 November 2026. The parent authored and revised the design; neither reviewer
edited repository files, changed GitHub, ran builds/tests, or granted operator
approval. Their final results were delivered separately to the parent session.

| Source | Exact Git blob reviewed |
| --- | --- |
| [Proposed ADR-0025](../adr/0025-project-and-shared-memory.md) | `9e657d2bd81a413cecccbce19cd787a75d9dfab5` |
| [Product research addendum](2026-11-05-shared-memory-product-addendum.md) | `0775dc7f0120636b8b34689a2c50fd88ec7290ec` |
| [ADR index](../adr/README.md) | `268f5e212f83248c16317971d3555953be7bdcb0` |

These are source bindings, not delivery commits. The final reviewed source was
uncommitted at review time. The review does not imply implementation, runtime
qualification, source export consent, a release move, task design approval, or
execution readiness.

## Architecture review: PASS for a Proposed decision

Reviewer `SharedADRReview` returned **PASS**, confidence 0.8, on final snapshot
`2ED7` (the ADR blob above). Its final review found:

- The recorded operator choices are faithful: project plus deliberately shared
  memory, no sibling-bank route, proactive export suggestions, ask-by-default
  configurable exports, ordinary AM admission, automatic authorized shared
  consultation, and a dedicated repository configured once per computer.
- The once-per-install setting is private, applies across projects without
  per-project enrollment, and is not inherited by collaborators.
- An agent-supplied repository/path argument cannot claim an authorized session
  project switch.
- Operator confirmation does not silently release another collaborator's
  project learning; that multi-developer source policy remains unresolved.
- AM-initiated remote embedding/model sends are distinguished from guidance
  returned to an already authorized harness under its existing provider policy.
- Git raw-access limits, admission provenance, observed revocation/freshness,
  and source/metadata disclosure limits are represented honestly.
- The #246/#247 candidate briefs and join remain byte-identical to `175d21f`.
  Their addendum is non-gating and does not add acceptance requirements.

The reviewer explicitly assessed safety and consistency as a **Proposed ADR**,
not implementation readiness. Decisions 1–7 and the task-specific execution
contracts remain open gates.

## Task-map review: complete, no blockers to the proposed map

Reviewer `SharedTaskImpact` independently reviewed the ADR, related accepted
ADRs, the frozen research, the addendum, and the live affected issues. Its final
verdict on snapshot `2ED7` was **map complete; no remaining blockers**. Every
outcome maps to an existing stable work key; no new task is needed solely for
this clarification. The deliberate #253 re-scope preserves its identity and
history while recording the old prompting criteria as superseded, not completed.

| Existing work | Review disposition |
| --- | --- |
| #246/#247 research | Frozen research delivery may proceed independently; addendum/comments are non-gating. Signed release remains outstanding. |
| #255 runtime qualification | Existing scope unchanged; sharing compatibility notes change after ADR acceptance, before its execution contract is frozen. |
| #252/#256/#257 local experience/runtime | Independent local delivery remains intact; original qualification and design gates remain. |
| #200 terminal recall | Local acceptance remains independent; shared-scope wording needs reconciliation after ADR acceptance. |
| #55 shared repository/configuration | Existing key covers canonical shared source, private per-install selection, collaborator independence, access and freshness. Not Ready. |
| #57 export/admission | Existing key covers selected export behavior, trusted policy, admission provenance, source authority and lifecycle. Not Ready. |
| #253 ordinary shared recall | Existing key is deliberately re-scoped to authorized automatic local-plus-shared recall and shared semantic generations. Not Ready. |
| #56 integrated graph queries | Existing key retains actual local/shared acceptance, with real shared authority prerequisites. Not Ready. |
| #62 qualification | Existing key covers local/shared rebuild, deletion, authorization, outage, isolation and Git remnants. Not Ready. |

The task reviewer called out one product effect for the operator: allowing an
operator-chosen shared-learning repository, even with a single reader, narrows
ADR-0001's older exclusion of per-developer knowledge overlays. Personal profiles
and preference stores remain excluded, and project canonical authority is not
replaced. The ADR states that amendment explicitly; it is not disguised as a
no-op clarification.

## Resolved findings and remaining work

Earlier review rounds requested changes. The final revision resolves the old
foreign-bank route, ambiguous RFC 2119 negations, unscoped deletion promises,
transitive export/local-authority copying, audience binding, standing-policy
coverage, generation isolation, task ownership, one-time installation semantics,
Git access/metadata limits, and source-release/disclosure ambiguities.

The optional difference that the addendum summarizes approval as content and
destination while the ADR also names audience is non-blocking: the selected Git
repository defines its audience. The ADR remains the normative source.

At the time of the final technical reviews, operator architecture disposition,
immutable source publication, and source-bound tracker reconciliation remained
outstanding. No task was promoted to Ready or delivered by those reviews.
The original seven v0.0.15 tasks retain their separate independent acceptance,
integration, and signed-release requirements.

## Subsequent operator architecture disposition

The proposal and these reviews were published at
`be999d93ea41703182f0ec35798a01138bcef1a6`. The operator then selected
**Adopt the architecture**, explicitly preserving execution-design and release
gates. ADR-0025 is therefore accepted for architecture only, not implementation.

The operator additionally clarified: this is **not necessarily developer-specific;
it can also be developer-specific**. The accepted source makes per-install
configuration distinct from dataset ownership/audience: one operator, a team,
an organization, or an ecosystem may use the same dataset model. Multiple
authorized accounts/installations may select the same repository; developer
identity is not its storage or eligibility key.

The exact proposal blobs above remain the inputs to the recorded independent
reviews. The later acceptance metadata and explicit audience clarification are
subsequent source revisions, not a claim that Opus reviewed bytes before they
existed. Reconciled tasks require their own source-bound readiness review.
