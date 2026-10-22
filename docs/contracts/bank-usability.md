# A usable bank without generated state

A clean bank is usable when its bounded `MEMORY.md` router and applicable current
Markdown let a fresh agent understand the project and resume its work. Scope,
current/historical status, open questions and source pointers MUST remain visible
in those files. No active transaction marker may be present. An index, database,
MCP service, hook, federation catalog, cache or recovery journal MUST NOT be
required to understand that clean canonical bank.

This is a file-format and reader workflow boundary. It MUST NOT be presented as
proof that arbitrary prose is true, all memories are current, a model recalls
perfectly, or unlanded learning is visible on another machine. Governed identity,
admission, retrieval ranking, shared refresh and publication have their own
contracts and acceptance evidence.

## Bounded file-only resume

A fresh agent SHOULD perform these steps:

1. Select the intended repository explicitly and read its router first. It MUST
   check the schema/project ID, route purposes, lifecycle states, scope and read
   conditions. The router's time describes its map, not every target's revision.
2. It MUST stop if a lock or transaction marker is visible. A marker means the
   bank is interrupted, even if some target files look readable. A clean bank
   MUST NOT need a recovery journal; an interrupted bank can need one to repair
   a partial operation. Deleting generated state MUST NOT be used as recovery.
3. It SHOULD choose a small set of useful current files for the task, typically
   the brief and the applicable task/component context. It MUST NOT load every
   routed file, treat a different branch's lesson as universal, or use expired,
   superseded, revoked or deleted guidance as current merely because it exists.
4. It MUST check the selected files' size against its remaining read budget.
   The isolated walkthrough reads at most the 16 KiB router plus two targets of
   at most 4 KiB each. Larger files require a narrower request or the bounded
   disclosure operation; the reader MUST NOT silently rely on a truncated claim
   whose conditions/evidence may lie outside the portion read.
5. It MUST read current focus, relevant evidence/conditions, unresolved questions
   and next action from those targets. Unknown facts MUST remain unknown; the
   reader SHOULD follow stronger source links when needed for the current task.
   Missing source access MUST NOT be replaced by invented confidence.

Lifecycle/scope information is ordinary visible text in the router and context.
The recognizable context files can use concise prose without per-paragraph
records. Rich standalone records use their separate typed-record contract; the
minimum useful bank does not require an episode, procedure or search projection.
The [router contract](bank-router.md) describes required fields, useful omissions
and path/size checks. The [manual checkpoint](manual-checkpoint.md) describes a
single-file update without claiming mediated guarantees.

The existing read-only helper can check the structural baseline:

```sh
python3 -I scripts/bank_contract.py validate --repo /path/to/repository
```

The helper reports routes, scope and lifecycle, writes nothing and does not
create missing runtime state. Its success proves only `router-and-file-contract`
validation; the agent remains responsible for reading applicability and evidence.
Plain file tools can read the same map and targets when the helper is absent.
An unknown schema or interrupted fixture MUST fail instead of being silently
repaired or interpreted as the supported schema.

## What survives a reset or clone

Project Markdown and its source links travel through ordinary Git commits and
clones. A copied bank can also be inspected without Git metadata. Removing
memory-generated state MUST leave its canonical file bytes, IDs, conditions,
source references and useful content intact. It MUST NOT require a personal
profile or per-developer private bank to resume. Git's own repository/index
metadata is distinct from a disposable memory search index.

The isolated acceptance scenario commits a useful bank, clones it, removes all
memory config/state/data/cache/backup roots and resumes from the current router
and two small applicable files. The expired branch route stays visible as
history and is excluded from the selected current file set. A second scenario
adds interruption markers after deleting state and verifies refusal with every
canonical byte preserved. These checks establish clean-bank file usability;
concurrent sharing and recovery of actual torn writes are tested separately.
