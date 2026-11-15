---
id: ADR-0027
status: accepted
approval-scope: operator-selected managed graph direction; technical profile, licensing and v0.0.16 execution gated on G1
date: 2026-11-08
amends: [ADR-0024]
related: [ADR-0017, ADR-0023, ADR-0025]
---

# ADR-0027: AM-managed core Memgraph

## Why and approval boundary

The operator selected **“Keep AM-managed core Memgraph”** after an explanation of
Memgraph Community's amended Business Source License (BSL) and its distinction
from Enterprise licensing. AM still needs a substantive graph database,
embeddings and the SSG runtime learnings selected by ADR-0024. Replacing those
capabilities with links or literal-only retrieval is not this decision.

The core/MAGE research identifies a smaller dependency surface in core and
observed bounded graph/vector/template capabilities. It does not establish
accepted ANN scoring, independent retrieval quality, platform support or legal
permission. Both examined arm64 engines share the observed signed-cosine defect.

This record follows ADR-0026's existing-task refinement convention: it amends
existing outcomes, not a duplicate implementation story or a new release scope.
Operator approval covers the graph deployment direction only. Independent
technical review, exact profile qualification, licensing disposition, implementation
acceptance and signed delivery remain separate gates.

## D1: Select managed core; retain the historical reference

AM's managed graph direction SHALL be **core Memgraph acquired from upstream by
explicit setup**, not MAGE bundled in the portable skill or an operator-managed-only
substitute. The initial qualification candidate is core Memgraph 3.10.1 at
`docker.io/memgraph/memgraph@sha256:042c8642954a7bde971a9c7683c8b1360e7fcf129a5fae36d63f12449e7154aa`.
G1 MUST bind architecture-specific manifests, executable/dependency identities,
actual settings and supported execution environments before approving a profile.
The candidate digest is not proof of compatibility or a production default.

ADR-0024 R1's MAGE/9341 stack SHALL remain the historical SSG measurement
reference. Its raw observations, failures, opened holdout and erasure limitations
MUST remain intact. Core MUST receive separately frozen qualification; it MUST
NOT inherit MAGE's retrieval quality, portability, resource or acceptance claims.
The original #255 acceptance criteria MUST remain required, including exact
numerics, eligible exact-versus-ANN comparison, lifecycle/disposal, resource and
platform evidence, license BOM and final contract review.

The runtime SHALL remain dedicated to AM's checked project-bank-state generation,
with owned storage, fencing and whole-generation disposal as specified by the
[execution contract](../design/v0.0.16-graph-embedding-execution.md). This choice
MUST NOT authorize a shared user database, sibling-bank access or ungoverned
cross-project materialization. Canonical Markdown remains authoritative.

## D2: Separate licensing facts, product choice and permission

The exact v3.10.1 [BSL](https://github.com/memgraph/memgraph/blob/v3.10.1/licenses/BSL.txt)
grants copying, modification, derivative works, redistribution and non-production
use under its terms. Its Additional Use Grant permits production use only for the
stated Authorised Purpose, subject to provisos (a)–(c). AM MUST NOT describe the
current license as permissive open source or infer that every distribution or
hosted application is prohibited. The actual setup/distribution/hosted use MUST
receive a scoped permission assessment rather than a blanket assumption.

The exact [MEL](https://github.com/memgraph/memgraph/blob/v3.10.1/licenses/MEL.pdf)
§1.9 expressly excludes Community Edition and Open Source Software from its
defined Software. AM MUST NOT infer Enterprise acceptance merely because the
image contains the MEL PDF. Applicable Enterprise components/features and their
technical/configuration boundary MUST remain explicit; this exclusion is not
clearance of every component in the image.

The operator's selection MUST NOT be treated as third-party license permission.
Managed production acquisition MUST remain disabled until the specific upstream
setup/management model has recorded licensing clarification and the required
operator or counsel disposition under ADR-0024 R1. Explicit setup acceptance alone
MUST NOT be presented as curing an otherwise unauthorized deployment. Bundled
redistribution and hosted profiles remain separately gated. No vendor approval,
commercial license or legal clearance is claimed here.

## D3: Preserve embedding and lifecycle choices

This amendment SHALL NOT select or force an embedding model, llama.cpp, vLLM or a
reasoning model. The existing OpenAI-compatible base URL, optional key and
explicit-model-or-unambiguous-discovery contract SHALL remain unchanged. Documents
and queries MUST use the same configured embedding profile; an operator's
reasoning model MAY differ. Upstream llama.cpp remains a managed-serving candidate,
not a newly approved dependency.

Setup, download and service start MUST remain explicit. Recall and maintenance
MUST NOT silently acquire artifacts, start an unapproved service or restart a
service the operator stopped. Unavailable graph/vector capability MUST be reported
truthfully while the readable Markdown bank and literal CLI remain usable.

## Existing tasks and acceptance

- **#255 / ADR-0024:ssg-runtime-qualification** MUST reconcile the selected core
  direction into its contract, license/BOM assessment and separately frozen actual
  qualification. It MUST retain all original acceptance, historical negative
  evidence and unresolved gates; this ADR does not complete G1.
- **#257 / ADR-0024:project-graph-runtime** MUST implement the selected managed
  graph direction only after G1 and its own execution design are approved. An
  operator-managed-only implementation MUST NOT silently replace this scope.
- **#256 / ADR-0024:embedding-runtime** MUST retain the existing configurable
  embedding contract and its independent model/profile qualification.
- **#252 and #200** retain their guided/standalone recall scope and existing
  dependencies. This product choice MUST NOT move them to Ready or imply graph
  implementation, native integration or release acceptance.

The ADR, execution-contract and license amendments MUST receive independent Opus
review and source-bound work-accountability reconciliation. The existing work keys
and v0.0.15/v0.0.16 delivery boundaries SHALL remain unchanged.

## Evidence and status

[Core/MAGE research and limitations](../research/v0.0.15-ssg-runtime-licenses.md)
and the [proposed execution contract](../design/v0.0.16-graph-embedding-execution.md)
retain exact artifact and diagnostic references. The
[operator selection and independent Opus source/design PASS](https://github.com/robertelee78/agent-memory/issues/255#issuecomment-6722856189)
are recorded separately from execution. The selected direction does not approve
a particular embedding-serving profile. No G1, implementation, platform,
licensing or delivery pass is asserted.
