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

## Local current-file inspection

With `SKILL_DIR` resolved to the packaged skill directory:

```sh
python3 -I "$SKILL_DIR/scripts/bank_state.py" inspect --repo /path/to/project
```

The command validates the bank, reports each recognizable context or its omission
reason, routes and current source hashes, and optionally inspects local Git
context. It writes nothing and reads no memory-generated application state.
It does not assemble task context, admit a claim or prove semantic currentness.
A session MUST still apply [scope](../scope/GUIDE.md), lifecycle, source-authority
and trust checks before using a record as guidance.

Git facts are deliberately limited: `context_head` is the local repository HEAD,
and `index_entries` show current file paths in the index. Index membership does
not prove that the current bytes were committed, pushed, merged or published.
The inspector therefore reports `current_bytes_committed: not-assessed` and
`local-or-pending-sharing`, with publication and freshness unverified. This
means verification is pending, not that the helper proved a published version
absent. A session MUST use actual Git/publication/refresh evidence before
claiming another session can see the learning. Local Git inspection uses fixed
read-only plumbing, disables lazy fetch/network protocols and optional locks,
and does not execute remembered commands or refresh the worktree.
These switches follow the [Git command reference](https://git-scm.com/docs/git/2.53.0.html).

A plain untracked bank or unavailable Git metadata remains readable Markdown.
Missing Git, a failed index read or a different enclosing Git root MUST NOT be
presented as a verified audit trail or silently bind the bank to another project.
The session MUST retain local work and resolve identity/publication through the
appropriate operation. Ordinary source links remain usable independently of Git.

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
