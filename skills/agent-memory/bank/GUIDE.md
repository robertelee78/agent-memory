---
kind: domain
---

# The current Markdown bank

`memory-bank/` is the canonical readable project state, not a rendering that
requires a database or event replay. A session MUST retain useful current
knowledge, source links and applicability in the files themselves. Caches,
search indexes, projections and optional historical event journals MUST NOT
contain the only current copy of admitted learning.

A session MUST follow the [projection contract](../projections/GUIDE.md) when
building/checking derived references, and the [observed write protocol](../writes/GUIDE.md)
when applying mediated changes. Complete source observations and safe replay
are mechanical guards; they do not qualify an assertion as true or current.

## Small useful contexts

| File | Purpose |
| --- | --- |
| `projectbrief.md` | Stable goals and boundaries |
| `productContext.md` | User problem and intended experience |
| `activeContext.md` | Current focus, recent changes, open questions and next step |
| `systemPatterns.md` | Architecture and established patterns |
| `techContext.md` | Stack, setup, commands and constraints |
| `progress.md` | Validated capabilities, remaining work and known problems |
| `MEMORY.md` | Bounded router to contexts and optional records |

A session SHOULD start with the smallest useful set. It MUST explicitly omit
unsupported/irrelevant contexts with reasons rather than create empty files.
The router MUST remain within 16 KiB and 64 direct routes. Every canonical
Markdown file MUST be reachable under the supported routing contract. The
[offline guide](../offline/GUIDE.md) covers the actual manual setup/checkpoint
recipe; governed evidence-based initialization remains a separate operation.

A session MUST link stronger project evidence rather than duplicate requirements,
tracker state or ownership claims. It MUST distinguish observed behavior from
approved intent and preserve conditions and provenance. Git history is useful
baseline audit evidence when files have actually been committed. It MUST NOT be
the only indication that visible guidance is stale: the current file or linked
record MUST explain correction, revocation or supersession where needed.

## State loss and recovery

A session MUST scope cleanup to known memory-owned artifacts.
A fresh session MAY resume by reading the bounded router and applicable current
context with ordinary file tools. Removing disposable memory caches, search indexes and
optional historical journals MUST NOT change the current file contents or
require recreating them from a hidden store. A session MUST NOT describe cached
prose as authoritative when it disagrees with the current bank.

A multi-file recovery journal is different from an optional historical journal.
An interruption marker or visible lock still stops normal inspection/resume;
missing recovery evidence MUST NOT be bypassed by deleting the marker or
rendering a new starter. A clean bank has no unfinished transaction to replay.
Known interrupted operations require explicit finish/rollback recovery and its
retained journal. Ordinary manual checkpoints remain cooperative file edits,
not audited admissions or concurrency-safe mediated replacements.

## Domain loading and routes

A session MUST load this domain before its selected operation leaf and MUST NOT
preload sibling leaves for completeness. It MUST select the next operation from
actual observed state. The [shared model](../model/GUIDE.md) MUST load only when
a semantic question remains unresolved by this domain and operation.

A session MUST read [records/GUIDE.md](../records/GUIDE.md) when a lesson needs
typed provenance, validity, correction or revocation. Ordinary context prose
does not need metadata on every paragraph. Record format validation MUST NOT
be treated as admission or proof that its evidence and conditions are correct.

| Intent | Operation |
| --- | --- |
| Use the file-only workflow | [offline](../offline/GUIDE.md) |
| Inspect current bank files | [inspect](inspect/GUIDE.md) |
| Inspect typed record format | [records](../records/GUIDE.md) |
| Perform observed writes/recovery | [writes](../writes/GUIDE.md) |
