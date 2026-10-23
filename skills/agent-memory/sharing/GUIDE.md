# Repository memory and its Git boundary

One repository MUST have one canonical Markdown bank. Its default root is
`memory-bank/`. Memory owns durable learning; AWA owns projects, tasks,
milestones and delivery evidence, and Vox owns live coordination. A session
MUST NOT create personal memory, duplicate their changing state, or depend on
those services to read local memory.

Governed admission and its replay/recovery receipts MUST use this same resolved
bank root. A configured nested bank MUST NOT create a default `memory-bank/` or
accept receipt paths with that default prefix. The full
[admission receipt checks](../admission/GUIDE.md) still apply; location discovery
does not weaken project, operation, hash or interruption validation.

## Explicit local configuration

A session MAY configure a different repository-relative bank directory before
initialization or after an explicitly reviewed bank move:

```sh
git -C /path/to/repo config --local agent-memory.bankRoot 'docs/shared memory'
git -C /path/to/repo config --local agent-memory.authorityBranch main
```

Every public bank helper uses the same resolver, including identity, inspection,
checkpoint, audit and recovery. The bank directory MUST stay inside the actual
repository root, with canonical spelling and no hidden components, traversal,
symlinks or occupied file parents. The resolver MUST refuse competing banks or
an explicit setting that disagrees with an existing bank. These local Git
settings are not transferred by cloning. With no explicit location, the
resolver reuses the unique Git-visible `MEMORY.md` with `agent-memory/v1`
router metadata, including a custom location in a fresh clone. This discovers
a location, not a new identity or source authority. Project and developer
identity still require the [identity protocol](../identity/GUIDE.md); a new
machine MUST explicitly bind the existing project rather than create another.

Discovery inspects at most 64 possible router paths across working files, the
index and the current commit. A missing, emptied or damaged working/indexed
router MUST NOT conceal committed bank ownership. Git objects are read without
filters or networking, with a 16 KiB per-object bound; missing/oversized objects
require inspection rather than establish absence. Discovery locates a bank;
it MUST NOT restore or overwrite the damaged bytes. A staged location move
requires explicit reconciliation of the existing Git locations before mediated
initialization. When the selected router is absent, a
bounded physical inventory also checks for ignored banks; it MUST NOT adopt
them automatically. A router beside canonical context filenames, or multiple
canonical contexts without a router, requires explicit inspection even when
the locator is damaged and no current Git ownership remains. Unknown or damaged recognized routers, competing banks,
unreadable inventories and exceeded bounds MUST stop initialization for review,
not establish absence. Hidden directories and independent nested repositories
are outside this inventory. A session MUST explicitly inspect any known bank
there before choosing a location. Boundary labels, including the whole bank
prefix, authority branch and selected remote name, MUST pass listed safety
checks before they can appear in eligibility output.

Changing a setting does not move, merge or delete memory. A session MUST
preserve the original bank and resolve a move before mediated use; it MUST NOT
initialize a replacement just because a setting was lost.

Effective configuration is part of the pinned scope context. A prepared mutation MUST
refuse a changed resolved location or authority branch. Removing a local root
setting that resolves to the same unique existing bank does not change the pin. A session
MUST inspect and prepare a new proposal rather than edit its pin. An interrupted
operation MUST be recovered using its original configuration and proposal;
changing the configured bank is not recovery. Local state and journals remain
private and outside the checkout. No namespace is created per harness or person.

The configured authority branch MUST be an explicit safe Git branch name. If
unset, the boundary inspector uses the selected remote's locally observed HEAD,
or reports initial `main` as unverified. It MUST NOT infer shared authority from
the current feature branch. A local remote-HEAD observation is not server
freshness: actual exchange MUST fetch, verify the repository's current default
branch and follow permissions, checks, review and branch protections.

## Inspect eligibility without changing Git

With `SKILL_DIR` resolved to this package:

```sh
python3 -I "$SKILL_DIR/scripts/sharing_boundary.py" --repo /path/to/repo
```

The command resolves the actual project/developer, validates current canonical
Markdown, applies listed content-safety checks, and inspects Git's index and
ignore rules. `eligible_memory_paths` names only canonical bank Markdown, never
feature code, optional projections, local registries, caches, locks, transaction
markers, recovery journals, backups, raw transcripts or credentials. A session
MUST NOT publish such artifacts or use arbitrary files under the directory as
an eligibility list. It SHOULD keep their ignore rules in repository-level
configuration, without ignoring the canonical bank itself.

Ignored or untracked canonical memory, tracked excluded artifacts, unmerged or
nonordinary index entries, unresolved remote/default branch and unsafe or
unreviewable content produce explicit blockers. Unsafe content withholds the
path list as well as bodies. The inspector MUST NOT guess that a missing or
interrupted bank means no prior memory. Unknown locks/markers MUST remain in
place for [write recovery](../writes/GUIDE.md). Bodies and unsafe filenames are
not emitted by this operation. The path manifest is bounded to 64 KiB and each
content reader to 16 KiB; exceeding the content bound requires review rather
than partial eligibility.

The helper reports `local_boundary_ready`, roles and local evidence; it writes
nothing. `local_boundary_ready: true` MUST NOT be described as semantic admission,
remote freshness, committed current bytes, publication, permission to bypass
repository rules or an access-control decision. Index membership alone does not
prove current bytes were committed. A session MUST repeat source and identity
checks when actually preparing or landing a memory-only delta, retaining
conditions/evidence for observations from unfinished code.

Exit 0 means a completed inspection, which MAY report blockers. Exit 1 means
unresolved, unsafe, interrupted or changed state; no eligible result is supplied.
The session MUST resolve blockers/failures before exchange and preserve unrelated
code, staged work and branch history. This inspector does not commit, fetch,
push, merge or refresh memory. Until those operations are available and verified,
sharing MUST remain pending while useful local learning stays readable.
