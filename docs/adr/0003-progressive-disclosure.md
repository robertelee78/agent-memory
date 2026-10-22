---
id: ADR-0003
status: proposed
date: 2026-10-21
depends-on: [ADR-0001, ADR-0002]
---

# ADR-0003: Progressive disclosure for memory and skills

## Context

Loading every memory and every operating instruction into every session wastes
context and lowers adherence. Pure similarity search lacks a stable map and can
miss the one rule an agent needed to know existed.

`agent-work-accountability` demonstrates a stronger pattern: one registered
root skill exposes invariants and routing; domain guides add shared rules; an
operation leaf teaches one procedure end to end; lower-level mechanisms appear
only for diagnosis. Its full knowledge grew while its mandatory root context
shrank.

## Decision

Both stored memory and the memory-management skill SHALL use progressive
disclosure.

The exact layer responsibilities, route graph, leaf contract, and validator
requirements are specified in the
[progressive skill blueprint](../design/progressive-skill-blueprint.md).

### Memory disclosure

At session start the skill SHOULD expose only:

1. the repository `MEMORY.md` index;
2. active task or component context when confidently resolved.

No personal profile SHALL be loaded or maintained. Developer attribution does
not partition the shared project bank into personal namespaces. Cross-project
recall requires explicit selection of the relevant projects.

`MEMORY.md` SHALL be a bounded router, not a history dump. It SHALL name topic
files, their purpose, current validity, and retrieval keys. Detailed semantic,
episodic, and procedural records SHALL load only when the current task requires
them.

Retrieval SHALL first narrow by scope, validity, trust, and type, then rank by
keyword, semantic, temporal, relational, and task-stage signals. The system
SHALL support exact lookup without embeddings.

At turn boundaries the skill SHALL check for newly landed shared memory using
ADR-0012, refresh only memory paths, and load the relevant changed knowledge.
It MUST NOT interrupt an executing turn or require a background watcher. A
custom harness SHALL be able to invoke the same explicit skill/CLI boundary
operation. Client hooks are outside planned scope under ADR-0004. Retrieval
SHALL check applicability against the working branch, rather than assuming the
branch's code has advanced with imported memory.

### Skill disclosure

The portable skill SHALL contain exactly one registered `SKILL.md`. Nested
instructions SHALL use a non-registering filename such as `GUIDE.md`.

The instruction tree SHALL have four layers:

- the skill description used for discovery;
- a root containing global invariants, shared conventions, and a route table;
- domain guides containing rules shared by related operations;
- operation leaves containing triggers, exact steps, output interpretation,
  failure handling, and a diagnostic fallback.

Each operation leaf MUST link to and require its domain rules. Shared memory
semantics SHALL live in one model guide and load only when a lifecycle, scope,
authority, or trust decision requires it.

The first proposed domains are:

```text
SKILL.md
├── access/                 status, identity, scope, permissions
├── lifecycle/              install, update, uninstall, recover, version
├── recall/                 context, search, inspect, explain
├── capture/                observe, remember, checkpoint
├── govern/                correct, supersede, revoke, delete
├── maintain/               project, consolidate, compact, migrate
├── evaluate/               canaries, benchmark, security tests
└── model/GUIDE.md          types, lifecycle, authority, trust
```

Pack validation MUST check:

- exactly one `SKILL.md` under the portable artifact;
- every relative file and anchor link;
- reachability of each guide from the root;
- absence of cycles that force unconditional loading;
- required leaf sections and domain backlinks;
- frontmatter and client-neutral core rules.

Uppercase [RFC 2119](https://www.rfc-editor.org/info/rfc2119/) language SHALL
mark normative behavior throughout the skill and its guides. The pack validator
MUST detect mechanically invalid or ambiguous requirement forms where practical,
but MUST NOT pretend that keyword count proves semantic completeness.

## Consequences

- Agents receive a stable map without paying for the entire corpus.
- Memory organization can grow without making every prompt grow with it.
- Topic names, descriptions, and routes become part of the retrieval API.
- Validation must cover the navigation graph, not merely file existence.

## Alternatives considered

- **Load all memory files every session:** rejected for cost, distraction, and
  poisoning blast radius.
- **Vector retrieval only:** rejected because it provides no guaranteed map,
  deterministic routing, or exact procedural discovery.
- **Register every operation as its own skill:** rejected because recursive
  scanners can flood the system prompt and fragment one coherent lifecycle.
