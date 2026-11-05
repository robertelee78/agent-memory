---
id: ADR-0025
status: accepted
approval-scope: architecture-only; task execution requires independent design approval
date: 2026-11-05
amends: [ADR-0001, ADR-0008, ADR-0017, ADR-0023, ADR-0024]
depends-on: [ADR-0002, ADR-0005, ADR-0006, ADR-0011]
related: [ADR-0010, ADR-0012]
---

# ADR-0025: Project memory plus deliberately published shared memory

## Why and decision scope

The operator clarified the intended product on 5 November 2026: an agent working
in Repo 1 uses Repo 1's memory and a shared memory dataset. Repo 2 contributes
selected knowledge by export; Repo 1 does not gain access to Repo 2's
memory bank. The operator described this as a cleaner overall model and selected
**Proactively suggest** when asked whether agents should suggest exportable
lessons or wait for an explicit export request. The operator then selected
**Ask by default, configurable** for export authorization: exact-content approval
by default, with an explicit operator policy for bounded automatic export submission.
The operator selected **Normal AM admission** for authorized contributions:
ordinary evidence, comparison, and conflict checks, without mandatory second
human approval by a shared-memory maintainer.
The operator selected **Consult automatically** once the installation is configured
with a shared repository the operator is authorized to read. This replaces
per-lookup expansion prompting for that authorized shared scope; it does not
authorize reading a sibling project's private bank.
The operator selected a **dedicated shared Git repository**, whose location is
configured by each operator. Collaborators in the same project do not inherit
access to that repository or its configuration; they need both repository access
and their own AM stack configured to use the same shared repository.
The operator further specified **once-per-computer installation configuration**:
select the shared repository once for that user's AM installation on each
computer, using the operator's existing account access. That installation uses
the configured repository across projects without per-project connection steps.
The operator clarified that the dataset is **not necessarily developer-specific**:
it MAY serve one operator, a team, an organization, or an ecosystem. Per-install
selection is configuration scope, not dataset ownership or audience scope.

A live catalog of other repositories and request-time access to their private
banks solves a different problem. Treating that as the primary sharing model
creates unnecessary discovery, permission, availability, and disclosure coupling.
The shared dataset is deliberately published knowledge, not an automatically
searchable union of member banks.

The operator selected **Adopt the architecture** after the independent Opus
[proposed-architecture and task-map reviews](../research/2026-11-05-shared-memory-architecture-review.md).
This acceptance MUST NOT be treated as task-specific implementation approval,
production export consent, or permission to change an existing release scope.
Remaining policy choices and independently reviewed task designs remain gates;
source-bound task reconciliation MUST preserve that distinction.
Requirement terms use [RFC 2119](https://www.rfc-editor.org/info/rfc2119/).

## S1: Two authoritative memory scopes

One project's canonical `memory-bank/` MUST remain authoritative for that
project. An optional shared dataset MUST have its own durable admitted lessons,
identities, versions, conditions, evidence, and governance. An exported lesson
MUST NOT become shared merely by tagging or projecting its private source.

An agent's authorized recall scope MUST consist of its current project and the
installation's configured shared repository that the operator is permitted to read.
Shared-dataset access MUST NOT grant access to another project's bank, checkout, private
observations, source evidence, or private graph/vector store. From the current
project, AM MUST NOT open, query, index, or rank a sibling project's private bank
or derived store through its sharing or recall operations, including an explicit
cross-project expansion request. Deliberate exports reach readers only through
admitted shared records. This does not prohibit the operator changing the
session's current project through its established harness/checkout binding and
separately establishing that project's authority. An agent-supplied repository,
bank, or path argument MUST NOT count as changing that bound current project.

An exported lesson MUST be usable without opening its originating project.
Source attribution and evidence included in the shared lesson MUST themselves
be authorized for the destination audience. Private origin references MAY remain
in source-side private export bookkeeping; their existence, labels, paths, and
contents MUST NOT be disclosed to shared readers without authorization.

Shared lessons MUST retain material applicability conditions, caveats, trust,
and unresolved contradictions. Publication MUST NOT turn project-specific
experience into universally applicable guidance or claim that redacted evidence
was disclosed to or independently verified by the recipient.

Shared source content MUST remain readable and versionable Markdown. Graphs,
vectors, catalogs, and indexes MUST be rebuildable projections over authorized
sources, not an alternative writable authority or a route into private banks.

## S2: Explicit association and separate permissions

AM MUST resolve the session's current project and the shared-learning repository
selected in private per-user installation configuration before shared use. The
operator MUST be able to configure the repository once on each computer; that
selection MUST apply across projects using that installation without requiring
per-project enrollment. Installing AM from an account MUST NOT itself establish
access to the selected Git repository; the existing Git account still needs the
appropriate repository permissions.

The repository selection and credentials MUST NOT enter project-tracked memory
or settings inherited by collaborators. Operators working in the same project
MAY use different shared repositories or none. A collaborator MUST have both
access to the selected shared repository and their own installation configuration
selecting it before AM reads its lessons. Project membership MUST NOT grant that
access or automatically select another operator's dataset.
The configured shared repository MUST NOT be bound to one developer identity.
Different authorized people/accounts and installations MAY select the same
repository and consume the same admitted shared lessons. A team- or
organization-owned repository and an operator-only repository MUST both be
supported by the same dataset model; personal ownership MUST NOT be required.
Per-install private configuration determines selection, while repository
permissions and admission/governance determine the dataset's audience and use.

Bindings MUST use stable identities, not infer membership from directory
proximity, repository readability, recent work, or knowing another project exists.
Different sharing audiences MUST use access-separated source repositories rather
than claim per-record confidentiality among raw readers of one Git repository.
The Git host controls raw repository access. AM MUST NOT claim to prevent an
authorized raw reader from inspecting a clone or a raw writer from changing Git
content outside AM, as already required by ADR-0006. Separation of read,
contribution, and governance decisions here governs AM-mediated operations.
Contribute-without-read MUST NOT be claimed for plain Git; supporting it requires
a separately reviewed host/PR mechanism. Local admission has no implied remote
server: the reviewed design MUST specify the trusted admission execution point
and verifiable admission provenance. Readers MUST treat content lacking valid
admission provenance as unqualified, not accepted shared guidance.

The binding MUST distinguish permission to read shared lessons, permission to
release source-project content to a particular shared audience, and permission
to contribute to that destination. Each permission MUST NOT imply another.
AM-initiated embedding/model disclosure remains a separate permission under
ADR-0024. Shared content MUST NOT be sent by AM to a non-loopback embedding/model
endpoint until an independently reviewed shared-disclosure policy authorizes it.
Returning guidance to the current authorized harness follows that harness's
existing model/disclosure policy, as for project-local guidance; selecting a
shared repository MUST NOT select or authorize a new provider. Whether source
release terms additionally constrain downstream harness/provider disclosure
remains an explicit policy decision rather than a claimed enforcement guarantee.

Skills, hooks, and tool responses MUST explain when to consult local/shared
knowledge and how to propose a contribution. The shared CLI/core MUST enforce
scope and permissions independently of whether an agent follows that guidance.
A hook, prompt, model output, or agent-issued flag MUST NOT establish operator
approval or expand the authorized set.

The private configuration schema, credentials, shared-clone freshness horizon,
and configuration/access revocation handling remain execution-design questions.
One configured shared repository per user installation is the selected baseline;
per-project overrides or multiple shared targets are not requirements of this
decision. Local graph/vector placement remains a separate runtime question.

## S3: Deliberate export and destination admission

Agents SHOULD proactively identify reusable lessons at appropriate capture
boundaries. A suggestion MUST NOT itself export bytes, publish a record, contact
a new provider, or grant access. Routine suggestions SHOULD be bounded and avoid
repeated prompts for an unchanged candidate and destination.

An export proposal MUST identify the exact proposed shared content, destination,
audience, applicability conditions, and the provenance safe to disclose. Export
MUST require both source release authority and destination contribution authority.
It MUST remain distinct from destination admission: an authorized submission MUST
NOT automatically establish truth, novelty, or applicability.
The destination MUST use normal AM admission and comparison to evaluate an
authorized contribution; a separate human maintainer approval MUST NOT be a
mandatory second gate. Insufficient evidence or unresolved admission conflicts
MUST stop admission rather than silently becoming accepted shared guidance.
Escalation MUST reach the responsible operator through a reviewed workflow and
MUST disclose only destination content that recipient is authorized to read.
Contribution permission alone MUST NOT authorize correction, supersession,
pruning, or deletion of existing shared lessons. Those changes require separately
verified destination governance authority; missing authority MUST stop the change
and escalate without imposing a mandatory second human gate on otherwise
authorized ordinary admission.
These checks constrain AM-mediated changes, not arbitrary Git pushes. The
admission-provenance and governance-verification design belongs to #57.
For a multi-developer source bank, who may release another collaborator's learning
remains unresolved. For such a bank, operator confirmation or standing policy
MUST NOT itself establish authority to release another collaborator's learning
beyond independently verified source policy.

Unadmitted observations, quarantined material, secrets, and deleted content MUST
NOT enter shared memory merely because they are locally available. Rewriting or
summarizing such content MUST NOT bypass those exclusions. Source-side comparison
and destination-side admission MUST preserve distinct conditions and conflicts;
source approval MUST NOT authorize a misleading shared rewrite.
Received shared content and its derivations MUST NOT be re-exported to another
destination or wider audience, or copied into the reader's canonical project bank
as locally owned authority, without a separately reviewed derivation policy.
Read permission alone MUST NOT authorize those operations.

AM MUST ask for operator approval of the exact proposed content, destination,
and audience by default. An explicit operator-controlled policy MAY authorize
bounded automatic export submissions to selected destinations and audiences.
Its configuration MUST use a trusted operator channel; an agent MUST NOT grant
itself automatic-export authority.
Read access, retrieval prompt settings, and destination contribution permission
MUST NOT establish source release authority. A standing policy MUST NOT bypass
source eligibility, destination admission, or separate provider-disclosure checks.
Its scope, persistence, revocation, and enforcement receipt require a reviewed
execution design. The earlier ask/allow discussion about retrieval MUST NOT be
reinterpreted as export consent.

Exports MUST create explicit destination records rather than a live read-through
connection to private project banks. Automatic bank synchronization and automatic
upload of entire banks or conversations are outside this decision.
AM-generated shared commits, branch names, paths, and record metadata MUST NOT
encode an unauthorized originating project's identity or private paths. Before
the first export, AM MUST disclose that Git authorship/committer metadata and
published history are visible to readers of the shared repository.

## S4: Recall, graph/runtime, and independent local operation

Once the user's AM installation has a configured shared repository that the
operator is authorized to read, ordinary recall in any project using that
installation MUST consult applicable local and shared memory without requiring
per-lookup or per-project approval. Configuration or permission changes MUST be
enforced before further shared disclosure under the reviewed freshness contract.
This policy MUST NOT grant export, contribution, AM-initiated provider disclosure,
or access to another project's bank.

Shared recall MUST apply the same source qualification and progressive-disclosure
principles as local recall, including mandatory caveats and explicit related-body
fetches. Results MUST distinguish project-local and shared-dataset authority;
private originating-project metadata MUST NOT be substituted for shared identity.

Graph database, embedding model, and runtime learnings from SSG remain selected
capabilities under ADR-0024. This amendment MUST NOT reduce them to links or
literal search. Local and shared projections MUST stay bound to their own source,
policy, model, and generation identities; similarity MUST NOT grant access or
establish authority. Shared recall MUST NOT require a graph built from other
projects' private banks.

Local and shared datasets MUST remain separate isolation domains and generations.
A combined result MUST be a post-qualification merge labeled by authority, not
one ANN index or a traversal/ranking that crosses physical dataset generations.
The merge's ordering and mandatory-context budget require reviewed design.

Shared-service failure or an absent association MUST leave local memory usable.
Unavailable shared evidence MUST NOT be reported as an exhaustive no-match.
Revocation of shared read/contribution authority, and deletion or withdrawal of
a record in the shared dataset, MUST fence further AM-controlled disclosure and
invalidate affected derived generations. The effect of source-project deletion
on an independently admitted shared lesson remains a separate lifecycle decision.

This decision does not authorize automatic propagation of source edits, automatic
withdrawal of independently admitted shared lessons, or promises about copies
outside AM's control. Those are lifecycle choices, not consequences of read
membership, and MUST remain visible as unresolved rather than silently defaulted.

## S5: Effect on earlier ADRs and work

On acceptance, this ADR MUST replace ADR-0008's catalog-first proposal as the
primary cross-repository product model. Explicit identity and export governance
remain necessary, but a catalog of private project descriptions, repository
locations, and typed links MUST NOT be a prerequisite for consuming shared lessons.
The earlier member-project manifest MUST NOT require exposing private contributor
identities or repository locations to shared readers. ADR-0008's requirement to
preserve source project on every result is constrained to provenance authorized
for that audience. Its project-removal rule does not decide the fate of
independently admitted shared records; that lifecycle requires explicit design.
ADR-0001's requirement for explicit cross-repository retrieval is fulfilled by
the operator's explicit shared-repository configuration and verified read
authority, not repeated prompts during ordinary recall. Its related-project
scope MUST NOT be used as a direct sibling-bank lookup route.
This decision deliberately permits an operator-selected shared-learning repository
even when that operator is currently its only reader, and permits different
shared knowledge contexts for collaborators in one project. It amends the older
blanket exclusion of private per-developer knowledge overlays to that extent.
The shared repository MUST contain deliberately exported, admitted project
learning, not personal profiles or preference stores; those exclusions remain.
Developer identity MUST NOT silently select a private bank, and shared recall
MUST NOT rewrite or override the canonical authority of the project bank.

On acceptance, ADR-0023 D1-D3 and the local graph/embedding runtime remain
applicable. D4/P2's expansion into named foreign project banks is withdrawn as
the sharing route. Authorized shared recall uses the automatic consultation
policy in S4, not per-lookup expansion prompts. The earlier retrieval ask/allow
toggle MUST NOT become export consent or be required for authorized shared reads.
ADR-0017's foreign-scope approval wording is amended on the same boundary.
Direct sibling-bank access MUST NOT remain an explicit or implicit alternative.
The proposed `am recall-policy show|set --cross-project ask|allow` interface and
its foreign-expansion approval receipts are withdrawn, not completed. The
operator-controlled export policy in S3 is a distinct outcome owned by #57;
it MUST NOT retain a misleading retrieval-policy command or compatibility alias.

ADR-0024's source authority, native qualification, graph/model capabilities,
provider controls, and local-runtime independence remain requirements. Its
cross-project template and export references MUST resolve to deliberately shared
records rather than imply access to private originating banks.

The reviewed #246/#247 research MUST remain historical evidence. Their
[catalog](https://github.com/robertelee78/agent-memory/issues/246#issuecomment-6663620236)
and [export](https://github.com/robertelee78/agent-memory/issues/247#issuecomment-6665144764)
research-only dispositions MUST NOT be expanded into production implementation
approval. A separately reviewed addendum MUST identify changed assumptions
without rewriting the frozen technical-review candidate as if it had already
studied the new product model. This addendum is not a new acceptance criterion
or delivery gate for those research stories.

On acceptance, existing outcomes MUST retain stable work keys and history where
they still represent the same work. The task reconciliation MUST include:

- **#246/#247 (v0.0.15 research):** add non-gating clarification comments linking
  the amendment/addendum; preserve frozen research scope and release gates.
- **#55 (unscheduled):** own canonical shared Markdown/identity in the selected
  Git repository, once-per-computer private installation configuration,
  independent collaborator setup, read authority, and destination identity rather
  than catalog-first discovery.
- **#57 (unscheduled):** own proactive export proposals, ask-default and trusted
  standing-policy configuration, source release/contribution authority, normal
  destination admission/escalation, provenance, lifecycle, and safe local operation.
- **#253 (v0.0.16 target, gated):** deliberately re-scope the existing
  expansion-approval outcome to authorized local-plus-shared recall. Own ordinary
  non-graph shared recall and the shared semantic projection reusing #256/#257
  generation contracts per isolation domain. Integrate every recall surface
  delivered in that release, including structured search and #200 terminal
  search/related commands where available, with skill/CLI/native guidance and
  automatic consultation. #55/#57/#252/#255/#256/#257 supply the real authority,
  admitted sources, shared experience, and runtime prerequisites; local delivery
  MUST NOT depend in reverse on #253. The export-confirmation channel belongs to
  #57 and MUST NOT become a second retrieval gate. Preserve the original work key
  and explicitly record the withdrawn prompt/toggle criteria as superseded, not
  completed.
- **#56 (v0.0.16 target, gated):** own integrated local/shared-dataset graph queries
  through admitted records only. Unscheduled #55/#57 prerequisites remain visible;
  a local-only demonstration MUST NOT complete the retained shared outcome.
- **#62 (v0.0.16 target, gated):** qualify local/shared rebuild, deletion,
  authorization, outage and isolation with real A/B/shared controls.
- **#252/#257/#256/#200 (v0.0.16):** preserve independently deliverable local
  progressive recall, graph, embedding, and terminal outcomes. Shared integration
  MUST NOT become a new prerequisite for their existing local acceptance.
- **#255 (v0.0.15):** keep local runtime qualification independent of unscheduled
  sharing; record the new boundary in the execution-contract compatibility notes.

No feature release moves, new implementation commitments, or completed outcomes
are inferred from this task map. Tasks MUST include Why, What/design references,
and independently observable RFC 2119 Acceptance Criteria before execution.

## Acceptance criteria for the eventual shared-memory capability

- With independent A/B projects and shared dataset S, A MUST retrieve its eligible
  local and S lessons. B's unexported body, title, path, identity, and relations
  MUST remain undisclosed, including when B is locally readable.
- A lesson deliberately exported from B and admitted to S MUST be usable from A
  while B is unavailable. Recall MUST NOT open or query B or require its private
  evidence; disclosed provenance and caveats MUST remain accurate.
- Read-only membership MUST NOT permit contribution. Contribution permission
  MUST NOT release private source content. A proactive suggestion MUST produce no
  shared write or foreign/provider disclosure before the required authorization.
- Authorized ordinary recall MUST use eligible local and shared guidance without
  per-lookup prompts. Missing association or read permission MUST disclose no
  shared metadata. An explicit request from A to query B's private bank MUST be
  refused without opening B or disclosing its metadata, including when A's
  session supplies B as a repository, bank, or path argument.
- An agent-originated attempt to create or widen a standing export policy MUST be
  refused. Automatic export MUST still fail closed for ineligible content,
  out-of-scope destinations/audiences, missing release/contribution/provider
  permission, or revoked policy. Destination admission MUST still run.
- An authorized contribution MUST pass ordinary comparison/evidence/admission
  without mandatory second human maintainer approval. Insufficient evidence or an
  unresolved admission conflict MUST prevent accepted shared guidance and reach
  the responsible operator through the reviewed escalation workflow.
- Two operators working in the same project MUST be able to select different
  shared repositories or none without changing that project's tracked state.
  Giving a collaborator access to the project alone MUST neither expose another
  operator's shared-repository location/credentials nor enable shared recall.
  Shared recall MUST require the operator's own installation configuration and
  repository access under the reviewed clone-freshness policy. Removing that
  configuration MUST stop further AM-mediated disclosure. Observed authentication
  failure or revoked access, or expiry of the reviewed authorization-freshness
  horizon, MUST stop mediated shared disclosure. Offline revocation MUST NOT be
  claimed instantly observable; existing raw clones/history are outside this
  enforcement promise.
- Where a separately reviewed contribution-without-read mechanism is supported,
  comparison/escalation MUST NOT reveal unreadable shared bodies. Plain Git
  MUST NOT be advertised as that mechanism. Admission provenance and modification
  governance MUST be verified before shared guidance or changes are accepted.
- A user MUST configure the shared repository once per computer, then use it
  from independent project checkouts without repeating repository-selection
  setup. A second computer with its own AM installation MUST reach the same
  admitted shared lessons after equivalent configuration and authorized Git
  synchronization. Private installation configuration MUST NOT be copied through
  project Git.
- Different authorized developer identities and accounts MUST be able to select
  the same team/ecosystem repository and retrieve its same admitted record
  versions. Shared identity, storage, and eligibility MUST NOT require that
  dataset to belong to one developer. Different selection per operator MUST
  remain possible without making developer identity a dataset key.
- Source or destination denial, changed candidate content/audience, unauthorized
  attribution, unsafe source material, and stale approval MUST fail closed without
  weakening local operation. Exact confirmation and receipt semantics MUST be
  independently designed and reviewed before their implementation becomes Ready.
- Actual graph/vector rebuild, policy revocation, service outage, and source or
  destination deletion MUST demonstrate the reviewed lifecycle contract without
  reading a private sibling bank or resurrecting withdrawn controlled content.
- Actual skill/CLI and primary native journeys MUST demonstrate local/shared
  recall guidance and a proactive export proposal. A text-only policy or synthetic
  permission echo MUST NOT substitute for those product-surface observations.

## Remaining PM/design decisions

1. Standing export policy scope, persistence, trusted configuration, and revocation.
2. Destination comparison, governance authority, safe escalation routing, and
   admission receipt design.
3. Combined local/shared result ordering, coverage reporting, and context budgets.
4. Per-install configuration schema, credentials, clone/authorization freshness,
   and access/configuration revocation for the selected shared Git repository.
5. Export correction, source deletion, withdrawal, destination retention, and the
   treatment of independent shared corrections and downstream derivations.
6. Source release terms and authority over remote-provider disclosure of shared
   content; shared derivation and re-export permissions.
7. Source release authority for multi-developer project memory, including lessons
   attributed to other collaborators.

These choices MUST be resolved through bounded PM questions and independently
reviewed task designs; this ADR MUST NOT invent operator answers.
