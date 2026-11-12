---
id: ADR-0026
status: accepted
date: 2026-11-06
amends: [ADR-0016]
related: [ADR-0009, ADR-0010, ADR-0012, ADR-0014]
---

# ADR-0026: Source-bound sharing performance without repeated page reconstruction

Accepted architecture for existing [#209](https://github.com/robertelee78/agent-memory/issues/209), work key `robertelee78/agent-memory:ADR-0009:bounded-24-session-stress`. The operator explicitly directed “yes, fix that” and clarified that eliminating repeated page rebuilds is an obvious engineering fix, not another approval decision. Fable independently passed the architecture and execution contract. This authorizes the bounded checked-page export and consumer migration, not a broader cache rewrite. Initial acceptance requires eliminating per-page whole-frontier rebuilds with identical checked-page content and unchanged safety boundaries; actual end-to-end latency will be measured, without an invented percentage promise. Full workload/platform qualification remains required. This ADR is renumbered from the reviewed proposed0025 because another session had already assigned0025 to project/shared-memory architecture; that accepted decision is untouched.

## Why

The [RCA and primary-source study](../research/2026-11-06-sharing-performance-rca.md) binds a completed24-session/10,000-seed/two-round workload at source33c95bb to76,620.196seconds on the operator's normally busy macOS host.964page requests consumed56,461.760summed invocation seconds, overlapping across workers. Every page calls the complete frontier builder again. Focused public-CLI profiles confirm each page repeats the same whole-bank observation, validation, parsing and component work as inspection; constructing the sparse page itself took approximately0.013profiled seconds versus roughly74seconds for the whole operation. This is not merely Python startup or network transfer.

Refresh apply/publication land are additional major costs. Their independent source/transaction checks have correctness purposes and cannot be removed because page export is faster. The RCA distinguishes complete-controller success from full daily-use qualification: the latter and the complete required fault matrix remain false. The old15-minute wish and proposed20%goal are not accepted constraints.

## Decision

### 1. Separate immutable disclosure from current authorization

Selected inspection MAY emit the exact already-qualified pages into an explicitly requested private artifact bundle, alongside the existing manifest. Pages SHALL preserve exact schema, bytes, ordering, hashes, source keys, edges and withholding decisions. This is materialization of one observed result, not a cross-operation cache of source truth. Hashes SHALL establish artifact identity, not applicability, currentness or permission to publish.

Fresh semantic review, publication prepare/land, refresh prepare/apply, original-index, scope/policy, authority and hold checks SHALL remain authoritative. An exported page MUST NOT substitute for those checks. Changed authority, local source, effective framing, lifecycle, hold state or recovery state SHALL invalidate the appropriate later operation rather than silently use a stale result. No global atomicity/currentness lease is claimed between independent commands.

The exact proposed CLI/codec/filesystem contract is the [execution design](../design/v0.0.15-209-checked-page-export.md). It MUST be reviewed with this ADR. Its private completion marker SHALL mean only complete artifact persistence. A marker MUST NOT be treated as proof acknowledgment, successful publication or current semantic truth.

### 2. Keep expensive safety boundaries; remove proven redundant derivation

The first implementation candidate SHALL be optional checked-page export, not a database, daemon, shared mutable cache, fused prepare/review, concurrent landing or larger disclosure cap. Existing `page-v2`/worklist consumers SHALL retain their semantics, including genuine negative controls. The optimized controller SHALL consume actual exported pages, validate every descriptor and derive its judgments from complete page items. Both comparison arms SHALL use that same page consumer; only fetch mode may differ.

Repeated pure parsing/format validation inside an operation is a separately identified residual opportunity, not automatically included in the first implementation. Any later reuse proposal MUST bind immutable bytes, paths, validator/scanner definitions and flags; preserve filesystem-dependent authority normalization and result isolation; have finite budgets and safe fallback; and retain fresh source/index/policy/hold observations. It MUST obtain its own precise design/review before inclusion. No persistent source-view or recall-graph cache is authorized by this record.

The separately reviewed parsed-companion refinement uses the existing operation-local `bank-format` memo for a uniform pair of the format report and internally produced parsed records, including when the caller requests only the report. Capacity and the empty-output argument rule are checked before reuse; callers receive isolated results, and filesystem-dependent router normalization is rechecked on every hit. Exact complete map identity and the existing entry, map and byte budgets remain unchanged; larger companion values may cause eviction or uncached fallback. This reuses no source observation, authority decision or semantic judgment. Fable's technical design review passed this narrow refinement; source review and the required functional and performance qualification remain separate, and no performance outcome is established by that review.

### 3. Artifact safety and boundedness

The exporter SHALL use a new absolute output directory outside canonical/runtime roots, owner-private checked parent directories, exclusive no-follow creation and bounded writes using existing private-workspace conventions. Both outputs SHALL be absent, distinct and non-overlapping. Symlink/traversal/output collision, permission/disk failure, partial artifact, missing/duplicate/reordered/corrupted/stale pages and response loss SHALL have explicit refusal/recovery behavior. Existing64KiBpage limits and whole-operation caps SHALL remain. No source/edge/hold truncation or empty-success fallback is permitted.

The exporter SHALL write and verify every page and normal manifest before its final marker, recheck sources and known-hold inventory, and check the recovery-frontier token captured **before the snapshot read inside build**. This token requires the accepted #244 API; its absence blocks integration rather than permitting a fabricated equivalent. Later fresh validation remains required. fsync conventions cover process crash/response loss; a full power-loss durability guarantee is not asserted.

### 4. Measurement and acceptance are distinct gates

The implementation MUST NOT be declared faster by subtracting cumulative page duration from full wall time. Instrumentation SHALL record per-command monotonic start/end and enclosing setup/drain wall, allowing schedule-aware comparison. The frozen baseline/candidate SHALL use the same runtime, workload, authored fixture identities, four-worker schedule, controls and page consumer except declared fetch-mode differences. A source/runtime change not in the optimization diff requires a new comparable baseline; the completed33c95bb/Python3.15run remains a diagnostic operating point, not automatically the denominator for a later integrated product.

The operator's normally busy machine SHALL remain an eligible target environment: quiet-host tuning, stopping unrelated work, pinning priorities or discarding slow runs is not a prerequisite. Host/software/load and overlapping workload evidence SHALL be retained. Repeated focused causal diagnostics MAY use controlled disposable inputs, but their timings MUST NOT replace the complete shared-host scenario. No statistical-confidence claim follows from one pair. The operator authorized the bounded engineering fix without another numeric-target approval round. Its structural criterion is one complete build per selected inspection and no whole-frontier rebuild for exported-page consumption, preserving exact content and all fresh authorization checks. Numeric end-to-end targets and final repeat/platform protocol remain separate qualification decisions, not prerequisites to implementing this explicitly authorized fix. No20%,15-minute or statistically significant speedup is promised.

Both macOS and Linux required cells SHALL preserve all24sessions,10,000seeds,two rounds,independent clones/developers and all declared semantic/recovery controls. Functional and performance results SHALL be reported separately. Full daily-use/fault-matrix qualification requires its own complete evidence; compatible-round completion alone is insufficient. Existing failures and apparatus errors SHALL remain visible; no retry-until-green or asymmetric candidate-only timed work.

## Alternatives and consequences

| Alternative | Benefit | Why not the first choice |
| --- | --- | --- |
| Materialize exact checked pages | Eliminates per-page whole rebuilds, no service/DB | Requires private artifact integrity, resource limits, crash behavior and clear freshness separation; selected first candidate, speedup unmeasured |
| One long-lived batched/streamed page process | Reuses observed result without file bundle | New process lifetime/cancellation/restart/consumer transport contract; possible fallback design, not needed initially |
| Persistent Merkle/CAS inventory and constructive traces | Can reuse parsing/derived work across commands | Dynamic framing, lifecycle, holds, actual policy/identity and mutable checkout/index require complete invalidation; hashes alone do not prove currentness |
| SQLite/MVCC or server-side retained cursor | Stable snapshot pagination | DB lifecycle/migration/connection retention and portability burden; does not remove authorization checks |
| Operation-local pure immutable source view | Targets repeated parsing without caching truth | Existing memo is bounded and intentionally conservative; broader reuse requires separate evidence/design rather than silently weakening checks |
| More workers or parallel landings | Possible throughput increase | Changes schedule/conflict rate, increases contention; does not remove duplicated work |
| Larger caps/shorter fixture/padded success | Makes superficial completion easier | Changes the problem or weakens protections; rejected |

The research compares Bazel/REAPI, Nix, dynamic incremental builds, Elasticsearch PIT, SQLite/PostgreSQL snapshot lifetimes, RFC9162/9111 and measurement literature. These methods are mature; no verified November2026novelty is needed to justify a simpler result-materialization boundary. We do not claim an exhaustive literature search or that this one change makes the workload a minutes-long check.

## Acceptance and dependencies

- RCA MUST cover dominant page/review/prepare/land/apply chains with actual focused runtime profiles and identify remaining attribution limits. Nested profile times MUST NOT be summed as exclusive percentages.
- Fable MUST independently review exact ADR, research, execution design and source-bound profile evidence. Minimum blockers MUST be resolved and the revised bytes re-reviewed where affected. A review is not operator approval.
- The operator has approved this bounded architecture and directed implementation. Full performance acceptance MUST still use an explicitly recorded qualification protocol and MUST NOT invent a numeric target or claim the structural fix alone completes #209.
- Integration MUST use the #244 recovery-frontier API and the coordinated comparison-v2 migration. #244 implementation can proceed independently; #209 final integration requires exact contract equivalence, source freeze and preservation evidence. Graph/model/recall studies are not a substitute for this source-bound correctness contract.
- Functional acceptance MUST exercise actual public inspection/export/consumer/review/publication/peer-refresh with exact learning/provenance/conditions/seed and unrelated feature/index/HEAD preservation, plus real filesystem/fault controls. Wiring/source-text/mock-echo tests are not acceptance.
- Performance acceptance MUST compare complete workloads under the approved protocol; lack of improvement or residual dominant costs returns to review, not silent scope/target weakening.
- Durable sanitized evidence, immutable source bindings and AWA reconciliation MUST precede Ready/acceptance claims. No story is closed by adoption of this ADR.

## Review and approval status

Architecture accepted by explicit operator direction after independent Fable PASS. [Review](../research/2026-11-06-sharing-performance-fable-review.md) and [reviewed-document hashes/model record](../research/data/2026-11-06-sharing-performance-fable-binding.json) retain the original proposed0025identity. Renumbering and this operator approval do not change the technical export contract. Implementation, independent acceptance, measured speedup and signed delivery are not established by this record.
