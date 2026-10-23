# Repository memory and its Git boundary

One repository MUST have one canonical Markdown bank. Its default root is
`memory-bank/`. Memory owns durable learning; AWA owns projects, tasks,
milestones and delivery evidence, and Vox owns live coordination. A session
MUST NOT create personal memory, duplicate their changing state, or depend on
those services to read local memory.

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
symlinks or occupied file parents. The resolver MUST refuse competing default
and configured banks. These local Git settings are not transferred by cloning;
a session using a custom bank MUST explicitly configure the same location in a
new clone before binding it. Changing a setting does not move, merge or delete
memory. A session MUST preserve the original bank and resolve the move before
mediated use; it MUST NOT initialize a replacement just because a setting was
lost. Project and developer identity still require the [identity protocol](../identity/GUIDE.md).

Configuration is part of the pinned scope context. A prepared mutation MUST
refuse a changed configuration, including a changed authority branch. A session
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
