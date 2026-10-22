# Optional records and bounded topic indexes

The six context files remain ordinary concise prose. A standalone file in
`topics/`, `episodes/`, `procedures/` or `history/` MUST use the shared
[record contract](../records/GUIDE.md), a topic-index contract, an optional
historical-event contract or the five-field safe tombstone exception. An
unrecognized schema or unstructured optional record MUST stop validation; the
reader MUST NOT invent metadata or rewrite existing content to make it pass.
Older prose topics MAY be inspected manually while preparing an evidence-backed
migration. Their bytes MUST remain intact until that change is reviewed.

Before adding memory, a session MUST inventory existing relevant records and
read their conditions, evidence and provenance. Duplicate copies are not another
lesson. A correction or consolidation MUST preserve stable identities and all
distinct source links, conditions and originating developer/harness/session
information. Format validation checks structure; it does not perform semantic
deduplication, admission, consolidation or publication.

## Placement and meaning

| Location | Intended content | Boundary |
| --- | --- | --- |
| `topics/` | Component knowledge, feedback, references and project decisions | Decisions SHOULD link to approved ADR/planning authority rather than copy it |
| `episodes/` | Bounded attempts, results and lessons with evidence | A session SHOULD retain useful stage-level learning, not full transcripts |
| `procedures/` | Admitted knowledge whose kind is `procedural-candidate` | MUST remain non-binding until reviewed skill promotion |
| `history/` | Inactive record explanations or optional empty-body events | MUST NOT contain the only current record or substitute for recovery state |
| `history/tombstones/<id>.md` | Exactly five safe deletion fields and no body | MUST remain repository-wide, content-free and terminal for the stable ID |

Shared standalone records MUST have an admitted or historical lifecycle state;
`candidate` and `quarantined` observations MUST remain outside the shared bank
until the governed admission operation accepts them. An active format alone is
not proof of admission. Independently useful branch discoveries MAY be admitted
with evidence and sufficient explicit conditions; memory landing MUST NOT imply
matching code on `main` or another revision.

Each record's project UUID, scope and lifecycle MUST agree with its route. A
version of an ID MUST occur at only one path; simultaneous active versions of
one ID MUST be reconciled. Inactive versions MAY retain the same ID with distinct
versions and explicit stale state. A deleted ID MUST NOT coexist with any record
body in the bank. The reader MUST inspect every tombstone-directory occupant,
including non-Markdown and hidden files, and refuse ambiguous deletion history.
History events use repository scope and an `active` route for the retained
artifact; the loader explicitly marks them as not current guidance. Duplicate
event IDs, content-bearing event bodies and a `deleted` label on an event MUST
be rejected. Deletion is represented by the safe tombstone, not that label.

## Bounded reachability

Every project Markdown file MUST have one unambiguous route from `MEMORY.md`,
directly or through a bounded topic index. The root router keeps its 16 KiB and
64-route limits. A topic index MUST:

- live under `topics/` and carry byte-zero frontmatter with
  `schema: agent-memory/topic-index-v1`, the bank's `project-id`, and `updated`;
- stay within 16 KiB and 64 routes, with no more than two nested index files;
- use the same five-column route table as `MEMORY.md` and link only optional
  files; the six context files remain directly routed from the root;
- retain an active index route, without treating its timestamp as the revision
  clock of the records beneath it.

Labels name canonical paths relative to the bank. Link destinations MUST be
relative to the index file so an ordinary Markdown viewer follows them correctly.
For example, `topics/index.md` labels `episodes/retry.md` and links to
`../episodes/retry.md`. The renderer computes this spelling. A session MUST NOT
replace those destinations with absolute paths, URL escapes or runtime-state
links. Route cells MUST NOT contain framing controls, including Unicode line
separators. Unicode names and ordinary spaces otherwise remain supported.

An indexed record MUST retain inherited scope conditions in its own metadata.
For example, an index scoped to `branch:feature` may route a
`component:src/retry` record only if that record also preserves the branch
condition. Moving the route later MUST NOT make the lesson universal. The loader
returns the scope chain; later context selection MUST check every applicable
condition together with lifecycle, validity, trust and authorization. Unknown
applicability MUST remain excluded until independently resolved.

Cycles, excessive depth, duplicate paths, missing targets, orphan Markdown,
project mismatches and contradictory scopes MUST stop validation without repair.
A detecting session SHOULD use the [authority guide](../authority/GUIDE.md) and
focused clarification when the files express a substantive disagreement.
Missing evidence MUST NOT be filled with invented requirements.

## Read-only operations

With `SKILL_DIR` resolved to the packaged skill directory:

```sh
python3 -I "$SKILL_DIR/scripts/bank_layout.py" inspect --repo /path/to/repo
python3 -I "$SKILL_DIR/scripts/bank_layout.py" model
python3 -I "$SKILL_DIR/scripts/bank_layout.py" render-index --input /tmp/topic-index.json
python3 -I "$SKILL_DIR/scripts/bank_layout.py" validate-index --file memory-bank/topics/index.md --path topics/index.md
```

Render accepts exactly `path`, `project-id`, `updated` and `routes`. Each route
has `path`, `purpose`, `scope`, `lifecycle` and `read-when`. It returns proposed
content, not a write. Inspect returns descriptors and source hashes from the
canonical bank; it preserves its Markdown, code, HEAD, index and runtime state.
The existing `bank_contract.py validate` and transaction validator use the same
layout algorithm. Exit 0 means valid layout/format or rendered index; exit 1
means refusal. These results MUST NOT be treated as admission, source review,
identity authentication, a current-guidance decision or a durable mutation.

A visible bank lock or transaction marker MUST stop inspection. The session
MUST route interruption to recovery rather than deleting state or regenerating
the bank. Optional history MUST NOT be the sole readable copy of current memory.
Removing a deliberately maintained historical artifact also requires updating
its route; a dangling durable link is an invalid layout, not permission for the
reader to silently discard it. A clean current bank MUST remain usable without
memory-generated caches, runtime journals or auxiliary event history.
