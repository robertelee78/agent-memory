# Shared-memory product clarification after federation research

## Scope and evidence boundary

This addendum records later operator input for
[proposed ADR-0025](../adr/0025-project-and-shared-memory.md). It does not rewrite
or add acceptance requirements to the independently reviewed #246/#247 research
candidate at commit
[`175d21fa26bbef41f8ec859b463876934e16db79`](https://github.com/robertelee78/agent-memory/commit/175d21fa26bbef41f8ec859b463876934e16db79).
The original [join record](v0.0.15-federation-research-join.md) binds those briefs
and their runnable evidence. Their technical observations remain observations of
the pinned same-project implementation, not proof of a working shared dataset.

The operator accepted delivery of the
[#246 research only](https://github.com/robertelee78/agent-memory/issues/246#issuecomment-6663620236)
and [#247 research only](https://github.com/robertelee78/agent-memory/issues/247#issuecomment-6665144764).
Those dispositions do not authorize production transfers, schedule #55/#57, or
satisfy any implementation/release gate. This addendum is non-gating for those
frozen research outcomes. The original briefs' pending-disposition headers
represent their historical candidate state; the linked later receipts record
the actual operator disposition.

## Later operator decisions

The 5 November PM discussion established:

- An agent in project A uses A's own memory plus a deliberately populated shared
  learning dataset. It does not obtain access to project B's private bank.
- Projects contribute selected lessons by export; the shared dataset is not a
  live aggregate or searchable union of contributing project banks.
- Agents proactively suggest reusable lessons. Suggestions do not publish.
- Export asks by default for approval of the proposed content and destination;
  an explicit operator policy can authorize bounded automatic exports. This is
  separate from retrieval prompt behavior and source/destination permissions.
- Authorized contributions use ordinary AM admission/comparison/evidence/conflict
  checks, without a mandatory second human maintainer approval.
- Authorized shared guidance is consulted automatically alongside local memory,
  without a prompt for each shared lookup.
- The durable shared dataset is a dedicated Git repository selected by the
  operator, not dictated by the collaborating project.
- The operator configures that repository once per user AM installation on each
  computer, using existing authorized accounts. No per-project connection step
  is required. Another operator in the same project needs both shared-repository
  access and their own AM installation configuration to use that dataset.

The final PM statement clarified the previous association question rather than
selecting either of its offered project-enrollment options. It must not be
recorded as per-project opt-in, project-tracked configuration, automatic account
provisioning, or automatic propagation of private installation settings.

## Which research assumptions change

| Earlier research assumption | Product-model clarification |
| --- | --- |
| Member-project catalog and foreign-project destinations | Operator-configured shared repository; private contributor-bank discovery is not a prerequisite. Stable shared identity and authorized association remain necessary. |
| Destination read permission means reading another project's exported projection | Read permission applies to admitted shared records. A's AM does not open B's bank or derived store. |
| Explicit foreign expansion and request-bound E approval | Explicit installation configuration and verified shared read authority permit automatic consultation. The old retrieval ask/allow interface and foreign-expansion receipts are withdrawn under ADR-0025, not repurposed as export consent. |
| Source project identity appears on every foreign result | Shared record identity/version is authoritative. Origin attribution and supporting evidence are included only when authorized and usable by the destination audience. |
| Federation membership/removal controls projected copies | Installation configuration and Git access determine mediated shared use. The effect of source withdrawal/deletion on independently admitted shared lessons remains an explicit lifecycle decision. |
| Federation steward approves contributions | Ordinary AM admission is selected; conflict escalation, verified governance authority, and admission provenance still need execution design. |
| Project-level association | Private, per-user installation selection applies across projects on that computer. Collaborators do not inherit configuration or credentials through a project. |

The research's separation of read, export, contribution/admission, and provider
permission remains useful. Neither an export confirmation nor a standing export
policy establishes otherwise missing source release authority. Multi-developer
source release policy remains unresolved.

## Guarantees retained, without overstating Git controls

Local memory remains independent of shared availability. Unavailable or stale
shared evidence is not an exhaustive no-match. Native eligibility, mandatory
caveats, evidence discipline, typed relationships, model/source/generation
bindings, and graph/vector rebuild/deletion controls remain requirements.

Plain Git gives authorized raw readers access to repository content/history and
raw writers whatever the Git host permits. AM-mediated admission/governance is
not a claim to intercept arbitrary pushes or editor operations. An execution
design must establish admission provenance, clone freshness, and observable
access-revocation behavior; it cannot promise instant offline awareness of host
revocation or recall already disclosed raw copies. Generated metadata must not
silently disclose private source-project identity, paths, or evidence.

## Work mapping and remaining gates

ADR-0025 contains the proposed source-of-truth task reconciliation. Existing keys
cover the outcomes: #55 configuration/shared source; #57 exports/admission;
#253 local-plus-shared recall; #56 integrated graph queries; #62 qualification.
The v0.0.16 local runtime/recall stories remain independently deliverable.

The addendum does not make any of those tasks Ready. Standing export policy,
source release authority, admission/governance receipts, safe escalation,
local/shared merge budgets, Git access/freshness handling, shared lifecycle,
provider disclosure, and derivation/re-export require their own reviewed designs.
No production export, shared recall, graph integration, or cross-machine behavior
was implemented or exercised by this addendum.
