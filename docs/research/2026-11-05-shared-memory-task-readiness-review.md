# Shared-memory task reconciliation and independent execution-readiness review

## Source and operator disposition

The operator adopted [ADR-0025](../adr/0025-project-and-shared-memory.md) for
architecture only. The independently reviewed snapshot source is commit
`5e7a5ffcdc5b2c25bf0cd65a03e741b74d652bc9`, blob
`1a7addbff7d46e48ba2db2f9d3b919e8d0e57912`.
The [architecture review](2026-11-05-shared-memory-architecture-review.md) and
[non-gating research addendum](2026-11-05-shared-memory-product-addendum.md)
separate the earlier technical reviews, later operator adoption, and the final
clarification that shared knowledge is not necessarily developer-specific.

Per-install configuration selects a dedicated Git repository once per computer
across projects. The dataset can serve an operator, team, organization or
ecosystem. Collaborators need their own configuration and repository access.
Recall consults local plus authorized admitted shared lessons automatically;
sharing never opens a sibling project's private bank.

## Independent Opus verdicts on published tasks

Two read-only reviewers used `anthropic/claude-opus-5-5:high` independently of the
parent author. `SharedTasksCoreReview` reviewed #55/#57/#56;
`SharedTasksRecallReview` reviewed #253/#200/#62 and the sharing-compatibility
notes in the still-proposed G1 execution contract. Both reviewed drafts, requested
changes, re-reviewed fixes, and finally fetched the live issue bodies and native
relationships. They changed no source or GitHub state and ran no tests/builds.

**All six published task writeups: PASS. None is execution-ready.** Each has a
problem/Why, proposed solution/What with design references, observable RFC 2119
Acceptance Criteria, validation, exclusions, supersession history, delivery
boundary and explicit readiness gates. A writeup pass is not execution-design
approval, implementation acceptance or delivery.

| Existing issue/key suffix | Independent writeup verdict | Execution gates at the reviewed snapshot |
| --- | --- | --- |
| [#55](https://github.com/robertelee78/agent-memory/issues/55) `ADR-0008:federation-catalog` | PASS | ADR-0025 decision 4; operator/Vince consultation disposition; own execution design; #246/#247; scheduling. No release scheduled. |
| [#57](https://github.com/robertelee78/agent-memory/issues/57) `ADR-0008:export-and-degradation` | PASS | #55/#247; decisions 1/2/5/6/7; consultation disposition; trusted confirmation, admission/provenance/governance/escalation/lifecycle/source-release design; scheduling. No release scheduled. |
| [#253](https://github.com/robertelee78/agent-memory/issues/253) `ADR-0023:expansion-approval` | PASS | #55/#57/#252/#255/#256/#257; decision 3; decisions 4/5/6 where applicable; trusted plain-terminal versus harness-bound selection mechanism; own shared-recall design. Conditional v0.0.16 target. |
| [#56](https://github.com/robertelee78/agent-memory/issues/56) `ADR-0008:graph-projection` | PASS | Scheduled/accepted #55/#57, #253/#255/#257; decisions 3/4/5; own shared graph schema/template/merge design. Conditional v0.0.16 target. |
| [#200](https://github.com/robertelee78/agent-memory/issues/200) `ADR-0017:standalone-semantic-search` | PASS | Complete G1/#255, #252/#256/#257, own execution design including trusted terminal/session binding. Shared-task prerequisites do not block local terminal delivery. |
| [#62](https://github.com/robertelee78/agent-memory/issues/62) `ADR-0009:optional-state-contracts` | PASS | #56/#57/#256/#257, #55/#253 indirectly, relevant decisions 1–7 and its own full qualification design. Conditional v0.0.16 target. |

The historical key `expansion-approval` is deliberately preserved for #253's
re-scoped shared-recall outcome. The old per-expansion prompt/toggle criteria are
superseded, not completed. No compatibility alias or separate export permission
is inferred from them.

## Scope, ownership and sequencing corrections

The reviews caught and the revisions fixed:

- **Acceptance cycles:** #55 accepts configuration/authority resolution and
  canonical source/version synchronization. #57 accepts real export/admission,
  its verifier and lifecycle state. Neither requires downstream #253/#56 recall
  to be delivered before its own acceptance. Full A/B/shared recall is accepted
  in #253/#62.
- **Shared generation ownership:** #253 owns shared-domain generation lifecycle,
  semantic/vector projection and qualified result merging. #56 owns the actual
  typed shared graph build/schema/templates. Local #256/#257 contracts are
  reused, not a second engine invented.
- **Raw Git pushes:** producer verification and reader/index rejection of missing
  admission provenance are explicit. Git access is not admission evidence.
- **Publication fidelity:** safe provenance, caveats, contradictions, conditions,
  non-universalization and no false redacted-evidence claims remain required.
- **Input/refusal coverage:** sibling repository/bank/path/template/reference
  substitutions and session-bound reference reuse cannot escape the bound project.
  Operator plain-terminal `--repo` selects one current project; distinguishing it
  from a bound-session call is an explicit design gate, not asserted solved.
- **Complete privacy/installation controls:** metadata/body/title/path/identity/
  count/relations, missing shared authority, distinct/common/no shared selection,
  and one user's second computer all remain observable controls.
- **No hidden gate replacement:** #253's obsolete approval-channel blocker was
  removed through AWA. Its actual header and native links name #55, #57, #252,
  #255, #256 and #257, plus the reviewed merge/freshness/lifecycle/provider and
  binding-mechanism gates. The reviewer verified this condition in the live issue.

The [G1 execution contract](../design/v0.0.16-graph-embedding-execution.md) remains
**proposed, unqualified and unfrozen**. Its independently reviewed sharing notes
reserve shared schema/parameters/bounds for #56, dataset namespace/freshness for
#55/#253, and additive authority/per-domain coverage schema for #253. G1 qualifies
project-bank generations; it does not claim a shared implementation or make local
acceptance wait for one. The sharing-note PASS is not full-contract acceptance.

## Source-bound and tracker verification

A deterministic `extraction-v3` refinement validation passed for the accepted
commit/blob: 12 existing outcome keys, 33 material obligation paragraphs, zero
validation errors. Independent review checked actual semantic coverage as well
as exact quotations. The logical source coverage did not create or reparent any
issue or create a new Project; ADR-0025 refines outcomes in their existing ADR
Projects, preserving the established amendment pattern.

Every open and closed issue was enumerated before writing: 164 non-PR issues
across three API pages. Each of the six target keys matched exactly one existing
issue. Primary historical bindings and existing ADR-0023/0024 refinements were
preserved; the exact ADR-0025 commit/blob was added as the authoritative refinement.
Human text outside managed blocks and AWA-owned record lines were preserved from
fresh live bodies. Titles/body/source bindings were read back after writing.

Affected fields were first marked **Reconciliation needed**. After source-bound
body readback, fresh draft/dry-run/apply reconciliation returned `verified:true`
for roots #53, #251, #199 and #58, with the affected sources **Current**.
The changed implementation tasks remain **Designing / Blocked**; zero tasks were
promoted to Ready. No existing release assignment, parent, Project, stable key,
implementation evidence or delivered history was changed.

The independent reviewers verified these live-body SHA-256 bindings:

| Issue | Published body SHA-256 |
| --- | --- |
| #55 | `472db2898281e5af4974bb44fc3ced3e460ddd068a6e1240d11f12751b8aceeb` |
| #57 | `ce04b9d6c8aeefc7222cada1398696a529d25124cae26a9c93d95352c1bee4fe` |
| #253 | `5c9a559a91b5fe821f18e4340c06391e2fde79f45f391c01b8ef2bb8acf0a0e9` |
| #56 | `f75ace1badc1516a8b2b300f9c4dfa99de8d313368da113c568faa2b854d57e8` |
| #62 | `4892a8d82a0f5c329f3af7f44d75d8317398d61df860247388bf0027f72ba732` |
| #200 | `38f362e754ebfb54b4a0138f55edfe7d5093c2ee7048a71d2372a4c1978751e5` |

## Unchanged v0.0.15 delivery obligations

#246/#247 retain the frozen research candidate `175d21f`, their independent
technical review and research-only operator dispositions. The product addendum
and clarification comments add no acceptance or release gate. #255's local
runtime qualification is unchanged; its contract's sharing assumptions are
reconciled without claiming G1 complete. #249/#244/#206/#248 are not narrowed.
No implementation, production sharing, export consent, signed release or task
completion is established by this architecture/task reconciliation.

## Subsequent source-export authority decision

After the recorded live review, the operator selected **Operator approval is
sufficient**: trusted approval or an explicit bounded standing policy supplies
source release authority for eligible project memory the operator may access,
including collaborator-authored lessons. AM does not require an additional
project-level export grant or allow source-project export restrictions to veto
that decision. Destination authority, normal admission and hard source exclusions
remain required.

This resolves former ADR-0025 decision 7; it is not a new execution-design approval
or consent to perform an export now. #57/#62 source-authority criteria and gates,
the research addendum, and all affected ADR-0025 refinement bindings require
reconciliation to the new source. The table and hashes above retain the exact
earlier reviewed snapshot rather than backdating the later decision.
