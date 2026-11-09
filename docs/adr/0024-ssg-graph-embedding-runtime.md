---
id: ADR-0024
status: accepted
approval-scope: architecture and G1 research; v0.0.16 execution design gated on G1
date: 2026-11-04
amended: 2026-11-09
amends: [ADR-0005, ADR-0008, ADR-0009]
depends-on: [ADR-0001, ADR-0002, ADR-0010, ADR-0011]
related: [ADR-0017, ADR-0023]
---

# ADR-0024: SSG-derived graph, embedding, and retrieval runtime

## Why and decision scope

Files preserve durable knowledge but do not by themselves provide useful semantic
ranking, typed multi-hop exploration, or a managed graph/model runtime. The
operator explicitly selected graph database, embedding model, and SSG runtime
learning as first-class AM capabilities, serving
[ADR-0023](0023-guided-progressive-recall.md)'s progressive-disclosure experience.
A link-only feature or a literal-search wrapper does not satisfy this decision.

This ADR refines ADR-0008's federation-only graph assumption: a within-project
graph is independently useful and MUST NOT wait for federation. Cross-project
queries remain a separate, fully governed capability. Graph/embedding support is
an intended v0.0.16 feature, while the readable Markdown bank and deterministic
CLI remain operational when those services are disabled or unavailable.
Optional-to-enable does not mean optional-to-implement in this roadmap.

The operator selected this product scope; independent
[Opus architecture review](../research/2026-11-04-ssg-am-architecture-review.md)
approved the architecture and G1 research actionability. Acceptance is not
implementation, measured improvement, legal clearance, or release evidence.
Requirement terms use [RFC 2119](https://www.rfc-editor.org/info/rfc2119/).

## R1: Concrete reference stack and v0.0.15 qualification

The integration MUST start from SSG commit
`9c4fd08c29f37bffe89ecdf75b069a6baeb8a5e0` and qualify this concrete baseline:

| Component | Reference selection | AM adaptation |
| --- | --- | --- |
| Graph database | Memgraph 3.10.1 in SSG's memgraph-mage image `sha256:e8e03640071c57de6a13f2f4c5f9ed1789797dc5e8e771bbe62203b20e1b8c27` | Dedicated AM-owned local service; graph and vector queries through reviewed templates |
| Embedding model | Qwen3-Embedding-8B Q8_0 GGUF, 4096 dimensions; weights SHA-256 `d20ddc71e8a5c4344f2343481e242233a997dc5eaff442427a945836c97b4deb` | G1 reference/candidate managed profile; production uses the configured compatible profile |
| Model server | llama.cpp build 9341 (`a578bb6b9`); SSG image `sha256:1ba15d0d9ecc71df4a5074b85b94664f8fda187cc57f9114d691e75fb2a0ff08` | Reference local serving option, not a mandatory dependency |
| Query/build runtime | SSG Bolt/Cypher, typed templates, source digest, model-keyed cache | AM-owned commands and source-qualified adapters, not a mandatory Rust rewrite |

The graph database and embedding capability are selected product goals.
Qwen and llama.cpp are SSG's observed embedding model and serving software, not
mandatory AM dependencies or an extra reasoning agent. AM MUST support an
operator-configured OpenAI-compatible embedding API URL with an optional API
key. The configured model MUST be used for both memory and query embeddings;
AM MUST NOT substitute a separate built-in model. G1 MUST compare local serving/
deployment options and recommend an AM-managed profile without restricting
bring-your-own compatible endpoints.

The image IDs above are observed SSG reference artifacts, not portable registry
references or proof of macOS/Linux compatibility. The v0.0.15 qualification
MUST resolve reproducible pull/build references, architecture-specific digests,
resource requirements, dependency/client versions, licenses, installation paths,
and supported macOS/Linux execution profiles before approving distribution.
SSG itself declares MIT OR Apache-2.0. Memgraph 3.10.1 declares BSL 1.1 with an
Additional Use Grant; this ADR MUST NOT label it permissive open source or infer
redistribution/hosted-service permission. The qualification MUST produce a
license bill of materials for the exact memgraph-mage image and dependencies,
compare a core-Memgraph deployment, and separately assess user-managed local use,
distribution, and hosted use. Recorded operator or counsel disposition MUST
decide permission for each deployment profile; technical review alone is not
legal clearance. Unresolved permission MUST block that profile, not silently
replace the graph with links or claim a legal conclusion.

R1's reviewed result MAY propose a different graph/model/server profile when
license, portability, resource, or measured quality constraints require it.
Any changed selection MUST receive an explicit ADR amendment and independent
review before dependent implementation becomes Ready. The graph/database/model
capabilities themselves MUST NOT be dropped as a supposed fallback.

## R2: Source authority, graph schema, and query contracts

Canonical Markdown, records, routes, conditions, evidence, and tombstones MUST
remain authoritative. The graph MUST be a substantive, rebuildable typed
projection, not a second writable memory bank. At minimum it MUST project stable
Project, RecordVersion, ContextSource, Route, Scope/Component, Condition,
Evidence/Source, and Origin identities from existing canonical fields plus
`derived-from`, `corrects`, `supersedes`, containment/routing, scope membership,
condition, evidence, and attribution relationships. Record `kind` MUST remain
typed. Nodes MUST retain project/record/version or tagged context identity,
source hash, and framing dependencies. Edges MUST retain their source assertion,
endpoints, conditions, provenance, and generation. Attribution and evidence
metadata MUST remain subject to disclosure policy. Namespace collisions between
records, context files, and sharing variants MUST be impossible. Template (2)
MUST traverse these typed relationships, not substitute similarity alone;
template (3)'s shared-entity schema belongs to #56's governed export contract.

New durable semantic relations MUST be declared through a reviewed canonical
schema/governance change before projection; inferred similarity MUST remain a
separate disposable relation with model/renderer/generation provenance. Existing
`derived-from` MUST NOT be overloaded as similarity because it drives deletion
closure. Sharing-review interactions are not domain-truth edges.

The runtime MUST support named, parameterized, bounded, read-only templates for:
(1) explanations/replacements and their supporting records; (2) related lessons
and prior attempts for a component/task; and (3) authorized cross-project shared
entities/relations when federation is available. Templates MUST have typed
parameters, content/version bindings, allowed read operations, hop/result/time
budgets, and output provenance. Arbitrary model-generated Cypher and write-capable
procedures MUST NOT be executed through recall. An agent MAY use an explicit
historical view, but inactive bodies MUST NOT be presented as current guidance;
deleted/compacted bodies MUST NOT be reconstructed from residual graph state.

G3 MUST deliver within-project graph capability independently of #55/#57.
Existing #56 MUST retain integrated within- and cross-project motivating-query
acceptance, depending on G3 and real catalog/export authority for its foreign
portion. A local-only demonstration MUST NOT close #56. Its conditional
v0.0.16 delivery MUST NOT block G3/G2/P1/#200.

## R3: Embeddings and the native ranking seam

AM MUST provide an actual embedding API adapter and persisted source/model-bound
vector store, not only keyword similarity. The connection contract MUST accept
an OpenAI-compatible base URL and optional API key and call its embeddings API.
A model name MUST be configurable when the provider requires one; a single
unambiguous model MAY be discovered from supported endpoint metadata. AM MUST
NOT guess among multiple models or claim that a chat-only endpoint supports
embeddings. URL/key/model configuration MUST suffice for an existing compatible
service; installing or managing a local model server MUST NOT be required.
Keys MUST remain in private configuration/secret storage, outside banks,
Git-tracked configuration, prompts, receipts, and logs.

Document and query embeddings MUST use the same compatible configured embedding
profile. The operator's reasoning/chat model MAY differ and MAY change without
re-embedding. An observable embedding model/profile change MUST create new
vectors and invalidate dependent similarity state; known incompatible vectors
MUST NOT be mixed, even when dimensions match. Available provider model/revision
evidence MUST be retained. An opaque provider can change weights under the same
alias: AM MUST report that stability as unverified and MUST NOT claim verified
weights or undetectable-drift prevention. G1 MUST define the identity/drift
contract, including whether fixed-probe diagnostics are appropriate and their
cost/tolerance limits. Equal probe results MUST NOT be treated as proof of
unchanged weights or universal vector compatibility.

Machine-readable semantic coverage/status and rendered status MUST identify
opaque aliases as `provider model identity not verifiable`. Reusing vectors
created at different times under such an alias MUST report reliance on the
provider keeping it stable; an explicit full re-embed MUST remain available.
A changed reported revision, dimension, normalization, configuration, or enabled
probe result MUST invalidate dependent vectors/similarity and trigger a
candidate rebuild. G1 MUST evaluate probe-at-fill cost and tolerance; probes
MUST NOT be described as proof of immutable weights.

Document text MUST use the record's own content plus declared necessary scope/
framing, without concatenating graph neighbors by default. Document/query
prefixes MUST be versioned profile configuration and MAY be empty; the reference
profile MUST use an AM retrieval instruction, not SSG's security-review prompt.
Endpoint/model identity, artifact digest where verifiable, dimension,
normalization, renderer, truncation/chunking, and prefix configuration MUST be
versioned. Every vector MUST bind the exact rendered source hash. Query vectors
MUST NOT be retained by default. Content-derived vectors and caches MUST be
treated as sensitive source-derived data.

AM MUST check authorization for model disclosure before sending any query or
source. Loopback local inference is the baseline. Non-loopback endpoints MUST
require explicit provider/destination approval and MUST NOT inherit federation
export approval. Model alias, response dimension, finite values, timeouts, and
partial batches MUST be checked. Failure MUST be explicit; literal fallback MUST
be labeled and MUST NOT become a successful semantic no-match.

**Operator clarification, 2026-11-09 — remember the embedding-service choice.**
After an explicit operator choice, AM MUST remember the approved service
destination and permitted memory-disclosure scope for that project across agent
sessions. A new session, lookup or ordinary permitted memory update MUST NOT
alone trigger another approval. AM MUST ask again when the destination or
permitted memory scope changes, and MUST stop relying on the choice when the
operator revokes it. This permission MUST NOT authorize shared-memory export,
sibling-project access or previously unapproved disclosure. Current source,
privacy, endpoint, credential and resource checks still apply; the trusted
approval channel and private receipt design remain G1 contract obligations.
The [operator disposition](../research/data/g1-embedding-choice-lifetime-disposition-20261109.json)
records this lifetime decision, not a present grant, implementation, runtime
qualification, production-licensing approval or release.

The ranking integration MUST preserve current native eligibility and source
rechecks. Semantic candidate generation MAY use already-authorized private data,
but final semantic ordering MUST occur after native scope/lifecycle/trust/
visibility/applicability/supersession/held-path qualification and before result
count and payload budgets. Current path selection loses external rank by giving
paths equal literal scores; sorting returned results afterwards MUST NOT be used
as a substitute for this seam. Exact/path/literal behavior MUST remain unchanged
when semantic ranking is not selected. Graph traversal MUST qualify both
endpoints and relation provenance before exposing paths, labels, counts, or
bodies. A hidden replacement MUST withhold obsolete guidance and preserve the
native withheld/clarification outcome without disclosing the replacement; it
MUST NOT be reported as a semantic no-match.

An eligible exact-cosine baseline MUST be measured before selecting filtered ANN
budgets. Fixed global ANN top-K followed by filtering MUST NOT be accepted as
complete eligible top-K or absence evidence. Recall, comparison/admission, and
three-view sharing MUST retain separate coverage/qualification contracts;
semantic top-K MUST NOT establish novelty or replace sharing review. Graph
expansion MUST NOT displace mandatory caveats/conflict groups in assembly.

## R4: Complete generations, freshness, deletion, and operations

A generation manifest MUST bind project/bank identity, canonical hashes and
framing, authority/context/policy revisions, producer/validator code and schema,
query templates, model/renderer/instruction identities, vector coverage, and
separate graph/vector capability completeness. Private pending/held-source
frontiers MUST be observed independently of unchanged canonical hashes. Source
currency, semantic coverage, and remote sharing freshness MUST be reported
separately.

Builders MUST create isolated candidate generations, verify required coverage,
and atomically publish the active manifest only after all required artifacts
are ready. Readers MUST use one generation and final native source/policy
rechecks. Interrupted graph fill, embedding fill, index creation, similarity-edge
creation, or pointer publication MUST leave the last still-qualified generation
usable or return an explicit degraded outcome; partial state MUST NOT claim
complete semantic coverage. This deliberately strengthens SSG's graph transaction
followed by separately attached vectors/similarity edges.

G1 MUST fix the physical project/build isolation unit and index query domain.
The baseline MUST NOT assume Memgraph Enterprise multi-database features are
available. A reviewed profile MAY use distinct project/candidate instances and
volumes with atomic active-generation routing, or another proven scheme;
label-only partitioning of one global ANN index MUST NOT establish isolation.
Queries MUST NOT rank across another project or an inactive generation.
G1 MUST also fix checkout/bank-state-to-generation binding, concurrent worktree/
clone build sharing, and the explicit unavailable/stale outcome when no complete
generation matches a reader's state. One project's branch MUST NOT serve another
branch's build as complete merely because their project UUIDs match.

Canonical changes and revocation MUST invalidate dependent derived results.
Deletion MUST purge prohibited bodies, vectors, relation payloads, caches,
database snapshots, write-ahead logs, owned volumes, and content-bearing model
server logs; old readers/builders MUST be fenced from republishing them.
Canonical deletion and pending derived erasure MUST be distinct states until
all owned persistent copies are verified erased or excluded under an explicitly
documented external-backup authority. Graph rebuild MUST honor current
tombstones/consent and MUST NOT resurrect content from a historical checkout
or cache. Cache authentication MUST NOT be confused with current source or
access authority.

A bring-your-own graph service MUST provide an explicitly authorized, dedicated
AM-owned store/namespace satisfying the reviewed isolation, generation, purge,
and ownership-verification contract, or that profile MUST be refused. AM MUST
NOT wipe a shared/user-owned graph or assume access grants erasure authority.
External embedding-provider retention lies outside AM's erasure authority:
approval MUST disclose its retention terms or their unknown status, and AM MUST
NOT claim provider-side erasure from a local purge receipt.

After explicit setup and endpoint/disclosure approval, AM MUST automatically
maintain graph projections and embeddings as eligible memory changes, at safe
boundaries without blocking canonical writes. It MUST reuse unchanged compatible
vectors rather than re-embed an entire bank for each change. Failed/unavailable
updates MUST remain pending/stale with actionable status; incomplete updates
MUST NOT become no-match or semantic-completeness claims. Explicit update/rebuild
and disable/purge controls MUST remain available. Content projected or sent for
embedding MUST first pass native operation-specific security, quarantine,
redaction, visibility, lifecycle, and disclosure eligibility. Secret-bearing,
quarantined, deleted, or unadmitted material MUST NOT be sent. Indexing eligibility
MUST NOT be represented as current-task applicability. Automatic maintenance
MUST use only the approved configured endpoint within G1-defined rate/batch/cost
limits and MUST NOT start managed services; stopped services leave updates pending.

AM MUST offer managed local setup and bring-your-own compatible services.
Managed graph/model status, start/stop, build/rebuild, diagnose, and purge MUST
be available through the public CLI. Setup/download/start MUST be explicit,
artifacts verified, endpoints local by default, and graph state private and
project-isolated. Existing approved remote embedding endpoints MAY be used
without a local model. AM MUST diagnose compatibility without starting,
stopping, modifying, or uninstalling bring-your-own services. Service/container
names, paths, ownership receipts, ports, locks, and uninstall MUST avoid SSG/
user/other-project services. Bank write locks MUST NOT be held during model
or database computation. Missing services, out-of-space, offline model
acquisition, cancellation, concurrent builders, and corrupt caches MUST preserve
bank/index/unrelated user state and produce actionable diagnostics. Removing
derived stores MUST leave ordinary local memory usable.

SSG's isolated code-analysis workspace and evidence-resolved critic are relevant
runtime patterns. The qualification MUST assess where transient code context
helps named AM queries and where evidence binding helps admission/review, while
keeping model judgments advisory. A code graph MUST NOT silently become durable
learning, and code-parser incompleteness MUST NOT prove absence. A critic, code
indexer, or second answer-generating model is not a prerequisite for the selected
saved-guidance retrieval experience.

## R5: Evidence and milestone boundaries

SSG's authored retrieval evaluation motivates experiments, not AM quality or
scale claims. The v0.0.15 #248 protocol MUST distinguish: semantic seed discovery;
graph expansion over the same seeds; ranking over the same candidate union; and
guidance exposure/cue overhead. It MUST retain the primary matched memory-on/off
coding-outcome study; mechanism controls MUST NOT inject memory into its off arm.
Ground truth MUST be independently authored, not copied from graph edges or
inferred similarity thresholds. The protocol MUST cover coding, investigation,
and orientation, mandatory caveats, abstention, privacy, and resource costs.

G1 MUST measure the reference/adapted scratch pipeline at
1,000/10,000/50,000 lessons: bytes/tokens, embedding throughput, graph/vector
build/rebuild cost, RAM/disk/model footprint, and eligible exact-cosine versus
ANN recall. It MUST freeze independent paraphrase/relationship suites, resource/
latency criteria, and operator disposition before dependent implementation is
Ready. Unavailable sizes/platforms MUST remain explicit production-profile
prerequisites, not passing measurements. G1 MUST NOT require the unimplemented
P1/G3 product experience to produce its baseline.

G2/G3/P1 MUST measure end-to-end applicable-guidance quality, wrong/unsafe
guidance, graph-path usefulness, cue/context overhead, disclosure, and latency
against those frozen suites and tolerances. Numeric performance pass criteria
MUST be frozen before a pass is claimed; no improvement is assumed from a graph.
SSG's all-pairs similarity construction MUST NOT be copied as AM's 50k default
without bounded-cost evidence. Typed graph queries and a real, persisted,
source/model-bound vector store are required capabilities; an ANN accelerator
MAY be selected when G1's eligible-recall and resource evidence justifies it.

Existing v0.0.15 #206 exact performance repair/approved thresholds, #209 full
24-session workload, and #244 source-preserving recovery MUST retain their
existing acceptance boundaries. Graph rebuilding MUST NOT replace raw-source
recovery or be inserted into the #206 paired performance denominator.
#246/#247/#248 remain research/protocol deliverables, not production federation
or coding-benefit claims. #249 native integration MUST NOT wait for this runtime.

## Work packages

### G1: v0.0.15 SSG runtime qualification and execution contract

Why: Selecting a graph/model by name is insufficient to install, operate, secure,
and qualify it on AM's supported systems, and SSG's runtime has concrete lessons
and incomplete-generation hazards worth testing before production integration.

What: Complete R1's source/license/portable-profile qualification in isolated
scratch environments; exercise actual reference graph/model/query build and
failure behavior; measure representative AM-shaped data; produce a source-bound
execution contract covering R2-R5, public CLI/schema/module interfaces, ownership,
generation publication, privacy, and evidence. Retain the runnable scratch
recipe and measurements, not a purported production implementation.

Acceptance Criteria:

- The brief MUST bind SSG and AM revisions and real graph/model/server artifacts,
  execution profiles, reuse/dependency licenses, resource costs, and observed
  macOS/Linux support or explicit unavailable prerequisites.
- Actual runtime smoke evidence MUST cover graph build/query, the reference
  model server and another real compatible embedding endpoint, own-text versus
  relationship-text controls, interrupted generation, and rebuild. API evidence
  MUST cover single-model discovery, ambiguous/chat-only refusal, batch/token
  limits, and truncation detection/declared limits. Unavailable profile/platform/
  endpoint/license evidence MUST remain an explicit prerequisite, not a pass.
- The contract MUST define graph schema/query templates, native rank handoff,
  complete generation protocol, provider disclosure, delete/fence behavior, and
  public setup/diagnose/purge contracts at implementation detail. It MUST fix
  physical project/build isolation and vector query domains without unapproved
  Enterprise features or cross-project/build ANN ranking.
  Checkout-to-generation binding, concurrent branch/clone handling, BYO graph
  ownership/erasure authority, provider retention, and profile identity/drift
  limits MUST be explicit in that contract.
- The contract MUST define OpenAI-compatible URL/optional-key/model selection,
  unambiguous discovery versus ambiguity refusal, same-profile document/query
  embeddings, model-change rebuild, local-default/approved-remote disclosure,
  private credentials, remembered per-project service/disclosure choice across
  sessions with change/revocation handling, managed versus bring-your-own
  ownership, and automatic source-change maintenance without canonical-write blocking.
- G1 MUST also freeze ADR-0017's machine response/exit-code schemas, supported
  profiles, and measured tolerances; provide the exact image license bill of
  materials, core-Memgraph comparison, and local/distribution/hosted assessments;
  compare serving options and recommend a managed profile; evaluate probe-at-fill
  cost/tolerance, automatic-maintenance rate/batch/cost limits, eligible exact
  cosine versus ANN recall, and SSG's transient code workspace/evidence critic.
- The study MUST retain reference/scratch measurements at the three named sizes
  or explicit missing prerequisites, independently authored query/relationship
  suites, and operator disposition of frozen resource/latency targets. It MUST
  NOT depend on production P1/G3 to establish these baselines.
- Independent technical review MUST decide compatibility with the reference
  stack or identify a required ADR amendment; operator/counsel disposition MUST
  separately cover the exact image/license deployment profile. Serving software,
  interface schemas, and remaining design decisions MUST be fixed before
  dependent implementation becomes Ready.
- This research MUST NOT widen #206/#209/#244, run real cross-project exports,
  or claim that the .16 feature or matched coding-benefit evaluation has shipped.

### G2: v0.0.16 embedding runtime and semantic ranking

Why: Literal matching misses useful lessons expressed differently; external
semantic ordering currently does not survive native path selection and budgets.

What: Implement R1/R3's reviewed real embedding service adapter, source-bound
vector generation/cache/store, and post-qualification native ranking seam.
G2 owns AM-managed embedding-model setup, verified acquisition, status,
start/stop, diagnose, and purge, using G3's shared generation/ownership contract.
Its acceptance surface is structured `am search --repo REPO --request REQUEST`
with an explicit semantic-ranking request policy frozen by G1. It MUST provide
that shared contract to later #200 and P1; #200 owns the human-friendly
`am search --semantic QUERY` and `am related` surfaces, not a G2 prerequisite.

Acceptance Criteria:

- Actual CLI queries MUST retrieve independently labeled applicable paraphrases
  through the configured compatible embedding profile/vector store, including
  examples with no required literal-word overlap and, when ANN is selected,
  eligible candidates below an unfiltered global ANN cut.
- Exact/path/literal behavior MUST remain unchanged without semantic selection;
  unknown conditions, held paths, hidden replacements, unsafe sources, and
  mandatory conflict groups MUST retain native behavior before final budgets.
- Model identity/dimension errors, disclosure refusal, offline/unavailable model,
  partial fills, stale source/policy, cache corruption, model upgrade, and
  concurrent deletion MUST be exercised through public operations.
- Actual compatible API journeys MUST cover a local endpoint without a key,
  an approved remote endpoint with a key, required-model selection or
  unambiguous discovery, ambiguous/chat-only endpoint refusal, same-profile
  memory/query embedding, changed-model rebuild, and automatic updates after
  memory changes. BYO use MUST NOT require AM-managed model installation or
  change the agent's reasoning model; credentials MUST remain undisclosed.
- G2 MUST exercise verified managed-model setup/download, explicit start/stop,
  status, diagnose, purge/uninstall, and ownership-safe BYO operation. Remote
  provider/destination approval MUST be separate from federation export;
  retention terms or unknown retention MUST be disclosed. Maintenance MUST NOT
  send secret-bearing, quarantined, deleted, or unadmitted content or start
  services. Model/database computation MUST NOT hold bank-write locks, and
  query vectors MUST NOT be retained by default.
- Actual journeys through public operations MUST show that a completed explicit
  embedding-service approval survives a new agent session, lookup and ordinary
  permitted update for the same project, destination and permitted memory scope
  without another prompt. A changed destination/scope or revoked approval MUST
  prevent queued and new sends under the old choice and require a fresh applicable
  approval before any further send. This MUST NOT grant shared export or access
  to another project's memories.
- Source/model/index identity, disclosure, resource costs, build latency, and
  retrieval quality MUST be retained at the agreed sizes and frozen tolerances;
  unavailable or literal-fallback results MUST NOT masquerade as semantic success.
- Frozen quality/resource passes MUST apply only to the measured reference or
  managed profiles. Other configured models MUST report their observed
  compatibility/qualification status and MUST NOT inherit an unmeasured pass.
- G1's reviewed profile/contract and G3 MUST be available before implementation
  Ready. Independent local destructive/fault qualification MUST prove that
  deleting all generated state leaves Markdown usable, rebuild matches current
  visible guidance, deleted/revoked content cannot resurrect, project scope
  cannot leak, and service outage cannot block checkpoint/resume.
- Independent implementation and local-fault acceptance MUST precede signed
  v0.0.16 delivery; #62's federation-dependent acceptance MUST NOT gate local
  G2/G3/P1/#200 delivery.

### G3: v0.0.16 project graph database and managed runtime

Why: Agents need real typed multi-hop knowledge navigation and a reliable graph
runtime inside one project, independently of unscheduled federation features.

What: Implement R2's local projection/templates (1)/(2) and R4's complete
generation, freshness, deletion/fencing, isolation, service ownership, and public
setup/start/stop/status/build/rebuild/diagnose/purge operations using G1's reviewed
database profile. Provide shared generation/ownership interfaces consumed by G2
and P1/#200; G2 owns model-specific managed setup/lifecycle. Template (3) and
foreign authority remain with #56/#55/#57/P2.

Acceptance Criteria:

- Actual CLI/database queries MUST demonstrate typed correction/evidence chains
  and component/condition/attempt navigation against independently labeled
  sources, without requiring federation or inferring truth from similarity.
- Interrupted candidate builds, pointer publication, source/policy changes,
  concurrent readers/builders, stale private hold frontiers, out-of-space, and
  unavailable services MUST preserve complete qualified generations or report
  honest degradation; partial state MUST NOT claim completeness.
- Independent local destructive/fault qualification MUST prove usable Markdown
  after generated-state removal, equivalent authorized rebuild, non-resurrection
  including engine logs/snapshots/volumes and stale workers, project isolation,
  and usable checkpoint/resume during graph/model outage.
- Installed setup/start/stop/status/build/rebuild/diagnose/purge/uninstall MUST
  respect verified artifact identity, explicit resource/download consent,
  ownership, supported macOS/Linux profiles, and unrelated user/SSG/other-project
  services.
- Graph projection upkeep MUST run automatically at safe source-change
  boundaries without blocking canonical writes or holding bank-write locks
  during database/model computation. It MUST apply native indexing eligibility,
  never start stopped services, and report pending/stale updates honestly.
- G1's actual reviewed profile/contract MUST precede implementation Ready.
  Independent local implementation and fault acceptance MUST precede signed
  v0.0.16 delivery; #56/#62/P2 federation gates MUST NOT block this local outcome.

## Existing integration stories

Existing #56 retains the integrated within- and cross-project motivating queries,
including R2's template (3), and depends on G3, #55/#57 and ADR-0023 P2.
Existing #62 retains qualification of the complete optional-store/federation
scope, including foreign leakage, consent removal, dangling references and
outages. G3/G2 independently qualify the same local safety invariants, so #62's
foreign prerequisites cannot block local delivery. No portion is completed by
G1 research or native catalog tests. #200 owns the standalone terminal surface.

## References

- [SSG source at the examined revision](https://github.com/cybersharkvin/ssg/tree/9c4fd08c29f37bffe89ecdf75b069a6baeb8a5e0)
- SSG `crates/ssg/src/{db,embed,build,graph,resolve}.rs`, `embed/cache.rs`, `query/`, `docs/eval-2026-11-03-retrieval.md`, `LICENSE`
- [Memgraph 3.10.1 license](https://github.com/memgraph/memgraph/blob/v3.10.1/licenses/BSL.txt)
- [ADR-0005](0005-admission-consolidation-retrieval.md), [ADR-0008](0008-project-bank-and-optional-federation.md)
- [ADR-0009](0009-contract-tests-and-fault-injection.md), [ADR-0010](0010-memory-bank-file-contract.md)
- [ADR-0023](0023-guided-progressive-recall.md), [ADR-0017](0017-standalone-semantic-search.md)
