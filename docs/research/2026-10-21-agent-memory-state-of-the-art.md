# State of the art: persistent memory for coding agents

Date: 2026-10-21

## Scope

This report asks what a shared memory layer for Codex, Claude Code, OpenCode,
and similar coding agents should learn from current research and production
systems. It focuses on memory that survives sessions and clients, not ordinary
conversation history or retrieval over a static documentation corpus.

Claims about research systems are taken from the cited papers and should be
read with their evaluation limits in mind. Several of the newest results are
2026 preprints rather than independently replicated findings.

## Research method and Codex capabilities

Codex does have web research capability. The CLI exposes web search with
`--search`; current Codex configuration distinguishes cached and live search.
The branded Deep Research workflow is a separate ChatGPT capability and was not
an independently callable tool in this environment. This review therefore used
Codex's live web access directly, prioritized official product documentation and
original papers, and audited the local AWA repository and its history in
parallel. See the official [Codex web-search documentation](https://learn.chatgpt.com/docs/web-search.md).

## Executive findings

1. **Cline's simplicity is a product property, not a missing feature.** Ordinary
   Markdown, a small named set of concerns, explicit update/resume behavior,
   and direct human editability produce a robust baseline that survives client
   changes and infrastructure failure.
2. **One-off project memory and ecosystem memory are different products.** A
   local Memory Bank should be the default. Cross-project relationships may
   justify a federated catalog or knowledge graph, but that layer must be
   optional and derived; the project bank must work when it is absent.
3. **Retrieval is not the whole memory problem.** A useful system must decide
   what to admit, how to represent it, when it is valid, what it supersedes,
   which scope may read it, and whether using it improved the downstream task.
4. **Coding memory must be evaluated by executable outcomes.** [VibeMemBench](https://arxiv.org/abs/2609.23570)
   found that directly injected, verified repository experience improved four
   of five held-out solvers, but eleven of twelve pairings of existing memory
   systems and solvers failed to beat the matched memory-off baseline. Useful
   experience existed; construction and retrieval failed to deliver it.
5. **Granularity must align with the agent's work decomposition.** [Subtask-level
   memory](https://arxiv.org/abs/2602.21611)
   experience outperformed whole-trajectory memory on SWE-bench Verified,
   improving mean Pass@1 by 4.7 percentage points over the memory-free agent in
   the reported experiments.
6. **Stale memory can be worse than no memory.** [TEPA's](https://arxiv.org/abs/2608.07429) controlled reversal
   experiments showed append-only and last-write-wins baselines falling below
   no-memory performance, while explicit revocation preserved current facts.
7. **Persistent memory is a durable attack surface.** Recent studies demonstrate
   delayed poisoning through external content and successful future-session
   steering after malicious instructions have reached memory. Formation order,
   scope, evidence, selection, and storage path are all security boundaries.
8. **The practical frontier is hybrid and hierarchical.** Strong systems combine
   a small always-visible index, typed records, temporal validity, provenance,
   scoped retrieval, background consolidation, and reusable procedures or
   skills. Flat transcript archives and ungoverned vector stores are weak
   foundations.
9. **Portable skills are procedural memory.** The progressive router → domain →
   operation structure in `agent-work-accountability` is directly applicable:
   expose the minimum stable map first and open detailed instructions only when
   the current operation requires them.

## A useful memory taxonomy

| Memory kind | Coding-agent examples | Preferred representation |
| --- | --- | --- |
| Working | Current task, selected files, immediate hypotheses | Ephemeral session context |
| Semantic | Project constraints, user preferences, known facts | Typed facts or compact profiles |
| Episodic | An attempt, failure, fix, review, or release outcome | Immutable, evidence-linked events |
| Procedural | How to perform a workflow or avoid a class of mistake | Versioned skill or rule |
| Relational | Components, decisions, people, dependencies, supersession | Links or a temporal graph projection |

This taxonomy follows the semantic/episodic/procedural distinction used in
LangGraph's current memory guidance and the broader cognitive framing in recent
agent-memory surveys. The important distinction is not the storage engine. A
semantic fact is not the same thing as semantic vector search.

## Evidence from current research

### Long context and recall benchmarks are insufficient

[LongMemEval](https://arxiv.org/abs/2410.10813) decomposes conversational memory into indexing, retrieval, and
reading, and tests extraction, cross-session reasoning, temporal reasoning,
knowledge updates, and abstention. It reported a substantial accuracy drop for
commercial assistants and long-context models over sustained histories.

[MemoryAgentBench](https://arxiv.org/abs/2507.05257) adds four competencies: accurate retrieval, test-time
learning, long-range understanding, and selective forgetting. Its main result
is negative: no evaluated approach mastered all four.

[MemoryArena](https://arxiv.org/abs/2602.16313) goes further by coupling memory to action across interdependent
sessions. Agents that were near saturation on conversational recall benchmarks
performed poorly when remembered experience had to guide later web navigation,
planning, search, or formal reasoning. The implication for this project is that
recall metrics are diagnostic, not acceptance criteria.

### Coding memory requires executable usefulness

[VibeMemBench](https://arxiv.org/abs/2609.23570) is the most directly relevant current benchmark. It uses 111
coding targets, 90 repositories, 3,634 historical trajectories, and executable
tests. A target is retained only when the injected experience improves the
reference solver, so the benchmark establishes that a useful prior experience
exists before testing a memory system's ability to construct and retrieve it.
Existing memory systems generally failed that delivery test.

[Structurally Aligned Subtask-Level Memory](https://arxiv.org/abs/2602.21611) identifies one likely cause: storing
an entire issue-solving trajectory as a single memory creates a granularity
mismatch. Similar issue descriptions can require different reasoning at
particular stages. Storage and retrieval should align with functional
subtasks—investigation, reproduction, localization, implementation, validation,
or review—rather than only with whole sessions.

[XRepoSkill](https://arxiv.org/abs/2609.36807) applies an even stronger standard to procedural memory. It contrasts
successful and failed trajectories for the same issue, extracts rules at the
points where execution diverges, attaches executable applicability predicates,
and retains a rule only when its behavior transfers across repositories. This
is a better model for promoting an episode into a reusable skill than simply
summarizing a successful transcript.

### Admission is a first-class decision

[Adaptive Memory Admission Control](https://arxiv.org/abs/2603.04549) models a write decision using future utility,
factual confidence, novelty, recency, and content type. [MemRouter](https://arxiv.org/abs/2605.00356) shows that a
small learned write-side router can outperform an LLM memory manager while
reducing admission latency in its matched conversational benchmark.

The exact models are not directly transferable to repository work, but the
architectural lesson is: **capture and admission are different operations**.
Hooks may capture candidate evidence cheaply; durable promotion should apply an
auditable policy. Assistant assertions, tool output, user statements, and
verified repository facts must not enter memory with equal authority.

### Time, contradiction, and revocation are core operations

[Graphiti](https://github.com/getzep/graphiti) models facts with validity windows and preserves the episodes from
which facts were derived. [TEPA](https://arxiv.org/abs/2608.07429) represents observations as keyed precedents and
revokes active precedents when newer evidence conflicts, retaining revoked
history for audit. Mem0's current production design similarly includes
supersession, merge, expiration, decay, and background synthesis.

For coding work, a memory such as “the test command is `npm test`” needs at
least two times: when the source claimed it, and when the memory system learned
it. A package-script change can make the fact invalid without making its prior
historical record false. Deletion, expiration, supersession, and security
revocation must therefore be distinct operations.

### Consolidation should be asynchronous and traceable

Letta's MemFS uses a Git-backed memory repository. Root files stay in the
prompt; indexed directories load on demand. Background “dreaming” agents
consolidate and reorganize memory in isolated worktrees. Mem0 Dream creates
higher-order pattern memories while retaining links to source memories, merges
duplicates, and supersedes outdated facts.

Background consolidation reduces latency and lets the foreground agent focus
on its task. It should produce proposed or derived memories with provenance,
not silently rewrite the primary event history. Consolidation must be
idempotent and reviewable.

### Governance and security remain unsolved

[GateMem](https://arxiv.org/abs/2606.18829) jointly evaluates utility, access control, and active forgetting in
multi-principal memory. No evaluated method was strong at all three. Retrieval
systems lowered cost but still leaked unauthorized or deleted information.

[Sleeper-memory](https://arxiv.org/abs/2605.15338) and
[persistent-memory-poisoning](https://arxiv.org/abs/2607.14611) work shows that content from a
document, webpage, or repository can induce a durable false memory that later
causes attacker-chosen behavior. MemForm broadens the problem from malicious
content to the complete formation interface: evidence, scope, selection, and
consolidation path can each be manipulated.

Accordingly, retrieved memory is **data, never instruction**. Procedural memory
requires a stronger promotion boundary than semantic or episodic memory.
Untrusted external content must not directly create user preferences, global
rules, or executable procedures.

## Production patterns

### [Cline Memory Bank](https://docs.cline.bot/best-practices/memory-bank)

Cline demonstrates the minimum viable pattern: human-readable project files
for brief, product context, active state, architecture, technology, and
progress. Its strengths are portability and inspectability. Its weaknesses are
eager loading, no typed lifecycle, no provenance requirement, and no formal
concurrency or revocation model.

The strengths are more important than that short description may suggest:

- the storage is also the user interface and recovery format;
- every client that can read Markdown can participate;
- the six files give memory a comprehensible shape instead of exposing a bag
  of chunks;
- `activeContext.md` and `progress.md` explicitly support checkpoint/resume;
- initialization and updating are intentional acts, so the user can see when
  the agent is maintaining memory;
- projects can evolve their own additional topics without adopting a service.

The appropriate improvements are bounded indexes, progressive topic loading,
evidence links, explicit uncertainty and supersession, safe concurrent edits,
and executable maintenance checks. Replacing these files with an opaque store
would discard the main reason the pattern works.

### Claude Code auto memory

Claude Code maintains a short `MEMORY.md` index and topic files per repository,
shared across worktrees. Only the first 200 lines or 25 KiB of the index are
loaded automatically; topics are read on demand. It distinguishes user,
feedback, project, and reference memories and records modification times. This
is a strong progressive-loading pattern, but it is client-local unless another
client is explicitly integrated with the same store.

### Letta MemFS

Letta is currently the closest production example of a memory-native agent
harness. Memory is a Git repository projected as ordinary files. Root memories
are always loaded, nested directories advertise themselves through `MEMORY.md`
indexes, and optional search is derived. Its versioning, hierarchical loading,
worktree-based consolidation, and separation of files from search indices are
directly relevant.

### Mem0

Mem0 offers the broadest ready-made integration surface across Codex, Claude
Code, and OpenCode. Its plugins use lifecycle hooks for capture and first-turn
recall, while MCP exposes CRUD and search. It distinguishes project, session,
and global scope and separates personal from shared project memory. Its current
implementation demonstrates useful adapter mechanics, but a cloud-first
service and automatic transcript extraction are not the desired authority
boundary for a local, auditable coding memory layer.

### Zep and Graphiti

Graphiti represents entities and relationships with temporal validity and raw
episode provenance, then combines semantic, keyword, and graph retrieval. It
is compelling when multi-hop relational queries justify a graph database. It
is too operationally heavy for the first version of a personal coding-memory
tool; the data model should preserve the option to derive a graph later.

### projectmem

projectmem is the nearest local-first coding-specific application: an
append-only plain-text event log, typed issues/attempts/fixes/decisions, compact
materialized summaries, MCP integration, and file-scoped warnings before an
edit. Its “Memory-as-Governance” framing is valuable. Its own evaluation is
primarily a feasibility self-study, so prevention of repeated failures still
needs controlled validation.

## What `agent-work-accountability` contributes

The accountability pack's progressive structure is more than splitting a long
file:

1. Client discovery sees only one `SKILL.md` name and description.
2. The root skill carries global invariants, the standing responsibility, and
   a route table.
3. Domain guides carry rules shared by related operations.
4. Operation leaves contain an end-to-end procedure: when to use it, exact
   commands, result interpretation, and a lower-level diagnostic escape hatch.
5. Leaves explicitly bind back to their domain, preventing selective loading
   from dropping domain invariants.
6. A shared model guide is loaded only when a phase, authority, or evidence
   question cannot be resolved locally.
7. Runtime state determines the next leaf. The instruction tree mirrors the
   lifecycle rather than presenting every operation at once.

The delivered split reduced the always-loaded root from 19,640 bytes to 11,166
bytes while expanding the full operational corpus from roughly 97.8 KiB to
161.4 KiB. Progressive disclosure enabled *more precise total knowledge* with
*less mandatory context*.

There are also lessons to improve on. The current pack validator confirms that
relative link targets exist, but it does not validate anchors, route
reachability, the one-`SKILL.md` invariant, or RFC 2119 coverage mechanically.
The new memory skill should make those acceptance properties executable.

## Proposed target architecture

The research supports a Markdown-first system with optional derived layers:

```text
optional ecosystem graph               explicit cross-project relationships
optional disposable search indexes     FTS/vector/graph acceleration
optional adapters and MCP              client ergonomics
────────────────────────────────────────────────────────────────────
portable skill + lifecycle CLI         shared behavior and maintenance
project Memory Bank in Markdown        durable product and human interface
Git + linked source evidence            history, review, and stronger authority
```

The horizontal line is an architectural boundary: everything above it is
optional. A fresh client must be able to resume work by reading the repository
bank and skill, even if all generated state has been deleted.

The project bank retains Cline's recognizable concerns and adds a bounded
`MEMORY.md` router. Git records document history; entries link to code, tests,
decisions, or user statements when those are the stronger authority. SQLite
full-text or vector indices are disposable projections. A temporal graph is an
optional federation projection for a real ecosystem of related projects, not
the default storage engine for an individual repository.

Memory access is partitioned at minimum by principal, repository,
worktree/session, and trust class. Global retrieval is explicit, never the
default. Standalone records and high-impact claims carry provenance and an
explicit lifecycle state such as active, superseded, revoked, expired, or
quarantined; ordinary prose should not be buried under metadata without need.

### Practitioner signal: two scales of memory

A trusted practitioner's current setup provides a useful reality check: their
one-off-project stack is essentially Cline's simple Memory Bank, while a
knowledge graph is reserved for the two project ecosystems they actively
maintain. This is not benchmark evidence, but it supports a strong product
boundary. Complexity should scale with relational need. A project should not
inherit the operational burden of ecosystem memory merely because the richer
model exists.

## Evaluation requirements

The initial implementation should not claim success merely because another
client can recall a canary. Acceptance needs four levels:

1. **Structural:** deterministic schemas, idempotent projections, migration,
   locking, scope isolation, deletion, and reproducible index rebuilds.
2. **Retrieval:** update, temporal, multi-hop, abstention, and selective
   forgetting cases with adversarial distractors.
3. **Coding outcome:** repeated repository tasks with memory-on/off conditions,
   executable tests, step count, token cost, and failure-repetition rate.
4. **Security:** poisoned documents and tool output, cross-project leakage,
   unauthorized global promotion, malicious procedural memories, deletion
   reappearance, and consolidation-order attacks.

Every evaluation should record memory candidates, admitted records, retrieval
results, prompt-visible context, agent actions, and executable outcome. This is
necessary to attribute a failure to admission, representation, retrieval,
reading, or action rather than calling all of them “memory quality.”

## Primary sources

- [Cline Memory Bank](https://docs.cline.bot/best-practices/memory-bank)
- [Claude Code memory](https://code.claude.com/docs/en/memory)
- [Letta MemFS](https://docs.letta.com/concepts/memfs/index)
- [Letta memory and dreaming](https://docs.letta.com/configuration/memory/index)
- [Mem0 paper](https://arxiv.org/abs/2504.19413)
- [Mem0 coding-client integrations](https://docs.mem0.ai/integrations/codex)
- [Graphiti](https://github.com/getzep/graphiti)
- [Zep temporal knowledge graph paper](https://arxiv.org/abs/2501.13956)
- [LangGraph memory overview](https://docs.langchain.com/oss/python/concepts/memory)
- [MemGPT](https://arxiv.org/abs/2310.08560)
- [A-MEM](https://arxiv.org/abs/2502.12110)
- [LongMemEval](https://arxiv.org/abs/2410.10813)
- [MemoryAgentBench](https://arxiv.org/abs/2507.05257)
- [MemoryArena](https://arxiv.org/abs/2602.16313)
- [VibeMemBench](https://arxiv.org/abs/2609.23570)
- [Structurally Aligned Subtask-Level Memory](https://arxiv.org/abs/2602.21611)
- [PROJECTMEM](https://arxiv.org/abs/2606.12329)
- [Adaptive Memory Admission Control](https://arxiv.org/abs/2603.04549)
- [MemRouter](https://arxiv.org/abs/2605.00356)
- [TEPA](https://arxiv.org/abs/2608.07429)
- [GAM](https://arxiv.org/abs/2604.12285)
- [GateMem](https://arxiv.org/abs/2606.18829)
- [Hidden in Memory](https://arxiv.org/abs/2605.15338)
- [Bad Memory](https://arxiv.org/abs/2607.14611)
- [MemForm](https://arxiv.org/abs/2610.14431)
- [XRepoSkill](https://arxiv.org/abs/2609.36807)
