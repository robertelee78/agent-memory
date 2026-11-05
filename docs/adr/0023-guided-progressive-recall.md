---
id: ADR-0023
status: accepted
approval-scope: architecture-only; task-specific execution design remains gated
date: 2026-11-04
amends: [ADR-0003, ADR-0004, ADR-0008]
depends-on: [ADR-0001, ADR-0002, ADR-0005, ADR-0011]
related: [ADR-0017, ADR-0024]
---

# ADR-0023: Guided progressive recall and explicit cross-project expansion

## Decision scope and Why

The operator selected this experience in the PM discussion of 4 November 2026:
lightweight cues, agent-requested saved guidance, and optional exploration of
related memories. Coding, investigation, and project orientation are all use
cases; none is the exclusive acceptance scenario. The operator explicitly wants
SSG's graph database, embedding model, and runtime learnings applied to AM.
[ADR-0024](0024-ssg-graph-embedding-runtime.md) specifies that machinery. This
record specifies the experience it serves, not a replacement for it.

The current bounded router and native reader preserve authority and conditions,
but do not deliver an integrated graph-backed, meaning-based exploration
experience. Dumping an entire connected component into context would defeat
progressive disclosure. Leaving important caveats behind optional links would
make the initial guidance misleading.

The operator approved this product direction; independent
[Opus architecture review](../research/2026-11-04-ssg-am-architecture-review.md)
approved the revised architecture. Acceptance does not establish implementation,
task-specific execution design, or delivery. Uppercase terms use
[RFC 2119](https://www.rfc-editor.org/info/rfc2119/).

## D1: Cue, retrieve, explore

AM MUST implement three separately callable stages through the shared CLI/core:

1. **Cue:** a bounded, project-qualified indication that relevant saved guidance
   may exist. A cue MUST NOT inject the memory bodies or a synthesized answer.
2. **Retrieve:** when requested, return applicable saved guidance with source
   project, record/version, source path, evidence, and conditions. AM MUST NOT
   require a second answer-generating agent to rewrite that guidance.
3. **Explore:** expose bounded pointers to related memories and a reason for each
   connection; fetch their bodies only on a subsequent explicit request.

The initial retrieval MUST include applicable answer-changing conditions,
corrections, superseding guidance, and unresolved contradictions. These MUST NOT
be hidden behind optional exploration. Native qualification MUST suppress
inapplicable or unsafe bodies; a caveat is not permission to reveal them. Where
safe guidance cannot fit with its mandatory context, AM MUST return an explicit
budget/clarification outcome rather than a misleading partial answer.

Cue metadata and relationship pointers are disclosures. They MUST undergo the
same project, visibility, security, and applicability controls appropriate to
their contents before labels, paths, counts, or relationship reasons appear.
An unavailable index MUST NOT become a false assertion that no useful memory
exists. A cue is a relevance suggestion, not proof of exhaustive discovery.

Exact/path/literal recall MUST remain available without graph/model services.
Graph and semantic interpretation availability MUST be reported separately from
source qualification and sharing freshness. Existing discovery traces MUST NOT
be fabricated from vector IDs or inferred connections.

## D2: Skills, hooks, and tool guidance

The integration MUST guide agent behavior, rather than merely make links
available. The portable skill and tool responses MUST teach when to retrieve
(past attempts, architectural decisions, recurring failures, unfamiliar areas),
when to explore (uncertainty, rationale, conflict, missing prerequisites), and
when to stop (sufficient applicable evidence for the task).

Native hooks MAY supply lightweight cues at supported lifecycle/turn boundaries;
they MUST NOT interrupt unrelated executing work, bypass permissions, or become
a second memory implementation. No particular mechanism is mandatory: the
shared skill/CLI path MUST work without hooks. Cue delivery MUST be bounded and
deduplicated for an unchanged task/source/policy context; task or source changes
MUST invalidate that suppression. The integration MUST preserve actual harness,
session, steering, cancellation, and source-completion attribution.

The v0.0.15 omp story #249 retains its approved native-integration scope. It
MUST expose the real supported extension points and keep the shared skill/CLI
route usable; it MUST NOT be made dependent on graph delivery. The new cue and
exploration experience ships in v0.0.16 and consumes those extension points.

## D3: Relationship meaning

Recorded relationships and inferred suggestions MUST be distinguishable in both
machine-readable responses and rendered output. Each recorded relationship MUST
identify its authoritative source and evidence. Each inferred suggestion MUST
identify its derivation/model/query generation and MUST NOT be represented as a
verified assertion or written into canonical memory automatically.

Similarity MUST NOT establish correction, supersession, causality, applicability,
trust, or absence. An agent seeking to make an inferred relation durable MUST
use ordinary admission/comparison/governance. Existing `derived-from` links MUST
NOT be repurposed as generic relatedness: AM's deletion closure follows them.

## D4: Cross-project scope and approval

The default query scope MUST be the current project. Cross-project exploration
MUST require an explicit expansion request naming destination project identities
and satisfying membership, access, deliberate-export, and destination policy.
Repository readability or a public repository MUST NOT imply export consent.

AM MUST ask the operator before each cross-project expansion by default. One
approval MUST bind one expansion request (origin, destinations, query/intent,
allowed operation, and policy revision), not an unlimited traversal. Bounded
paging/retry of that unchanged request MAY reuse its approval; changed scope,
query, policy, or a new expansion MUST require a new decision. Denial,
cancellation, missing identity, or unavailable approval MUST leave local recall
usable and MUST NOT query or disclose foreign metadata first.

Approval and changes to `allow` MUST use a trusted operator-confirmation channel
outside the agent's command stream, such as a native harness permission decision
or a separately operator-issued, scope-bound grant. A boolean flag, an agent's
answer, or allocation of a pseudo-terminal MUST NOT prove operator consent.
Agent-originated elevation MUST return approval-required; changing to `ask`
MAY be agent-invoked. When operator origin cannot be verified, AM MUST fail
closed. P2's independently reviewed task design MUST define the channel,
threat boundary, and receipt verification before its implementation is Ready;
it MUST NOT claim protection against a privileged actor that can modify the
trusted host itself. G1 and local recall MUST NOT wait for this federation gate.

An AM command MUST support temporary and project-persistent approval behavior:

- `am recall-policy show` MUST report the effective mode and scope.
- `am recall-policy set --cross-project ask|allow --scope session|project` MUST
  change only the approval-prompt policy. Session scope is the default when
  `--scope` is omitted; persistent project scope MUST be explicit.
- A session override MUST bind the real harness/session identity and MUST NOT
  affect parallel or later sessions. Plain-terminal callers MUST use an
  operator-attested, expiring AM session handle issued by that trusted flow;
  a handle MUST NOT be reusable by another session.
- A project override MUST be private local configuration bound to the stable
  project identity, not a Git-shared memory or another developer's preference.
- Session settings MUST override project settings; absent settings MUST mean
  `ask`. An explicit session `ask` MUST override a persistent project `allow`.

The command MUST NOT grant membership, export permission, provider disclosure,
or access to additional projects. `allow` means only that an agent may issue
explicit expansions within the already authorized set without another prompt.
Revocation/policy changes MUST invalidate outstanding approvals and derived
results before further disclosure. Approval receipts and settings MUST remain
outside canonical banks and MUST NOT persist raw queries unnecessarily.

Catalog/export research #246/#247 MUST refine these boundaries without treating
research as permission for real transfers. Production cross-project execution
remains dependent on the original #55/#57 feature outcomes; this ADR does not
silently schedule or complete those features.

## Delivery and work mapping

### P1: v0.0.16 guided recall experience

Why: Agents need relevant saved lessons and connected context without receiving
an unsolicited history dump or missing answer-changing caveats.

What: Implement D1-D3 through shared cue/retrieve/explore operations and portable
skill guidance, with native adapters using the supported #249 extension points.
The implementation MUST document the response schema, bounded cue deduplication,
mandatory-context assembly, and explicit related-memory fetch interface.

Acceptance Criteria:

- The actual installed CLI and at least the primary omp native workflow MUST
  demonstrate coding, investigation, and orientation journeys: small cue,
  explicit saved-guidance retrieval, then optional related-memory fetch.
- Not following a pointer MUST leave its body out of agent context; applicable
  answer-changing caveats MUST already be present with the primary guidance.
- Recorded relations and inferred suggestions MUST remain distinguishable;
  hidden endpoints, stale suggestions, and unavailable services MUST NOT leak
  metadata or produce false current/no-match claims.
- Skill/tool guidance MUST lead an agent through retrieve/expand/stop decisions;
  shared CLI operation without hooks, permission refusal, steering, cancellation,
  repeated boundaries, and supported-client regression MUST be exercised.
- ADR-0024 G1/G3/G2 and #249 MUST supply the reviewed shared contracts and local
  runtime. P1 MUST pass its own independent actual-surface and local fault
  qualification; it MUST NOT depend on #56, #55/#57, P2, or their cross-project
  release qualification.

### P2: v0.0.16-targeted expansion approval controls, federation-gated

Why: Useful cross-project connections must not become implicit knowledge transfer
or repeated permission prompts with no deliberate user-controlled alternative.

What: Independently design and implement D4's private policy/receipt model,
trusted operator-confirmation channel and threat boundary, explicit expansion
gate, and `am recall-policy` commands. Implementation MUST integrate existing
catalog, export, and revocation authority rather than invent an alternate
federation. The approval-channel design MUST be reviewed before implementation.

Acceptance Criteria:

- Fresh sessions MUST ask for each distinct expansion; denial/cancellation MUST
  perform no foreign lookup or metadata disclosure and MUST preserve local work.
- Actual CLI/native journeys MUST prove session-only defaults, explicit project
  persistence, precedence, parallel-session isolation, new-session reset,
  project/developer isolation, and terminal session-handle behavior.
- An agent attempting to grant itself `allow`, fabricate operator approval, or
  reuse another session's handle MUST be refused without foreign disclosure.
- `allow` MUST NOT widen the authorized set or transfer eligibility; revocation,
  changed requests/policy, retry/paging boundaries, and unavailable authority
  MUST be exercised with independent projects and explicit export policies.
- #55/#57 MUST supply production membership/export authority before cross-project
  execution is accepted. A same-project-only demo MUST NOT complete this story.

P2 and the complete #56 remain targeted to v0.0.16, but their release is blocked
until the operator schedules and accepts #55/#57. This condition MUST be visible
on the tasks; it MUST NOT block P1 or ADR-0024 G3/G2/#200 local delivery or be
hidden by a local-only acceptance claim.

## Consequences and alternatives

Graph/model machinery remains substantial product infrastructure, but its full
output is not mandatory prompt context. AM returns saved knowledge; the caller
retains task reasoning. Skills, hooks, and tools cooperate without making any
one harness the semantic authority. A fully synthesized default answer and
unrestricted automatic cross-project search are rejected for this experience.

## References

- [ADR-0003](0003-progressive-disclosure.md), [ADR-0004](0004-client-independent-integration.md)
- [ADR-0005](0005-admission-consolidation-retrieval.md), [ADR-0008](0008-project-bank-and-optional-federation.md)
- [ADR-0017](0017-standalone-semantic-search.md), [ADR-0024](0024-ssg-graph-embedding-runtime.md)
