# One configured repository bank and a memory-only Git boundary

The [portable sharing guide](../../skills/agent-memory/sharing/GUIDE.md) binds
canonical Markdown to one repository and keeps AWA's work evidence and Vox's
coordination separate. Local use requires neither service. There is no personal
memory namespace or cross-repository exchange.

The integrated admission contract retains its complete receipt schema and
proposal bindings while resolving bank and changed-path prefixes through this
same configuration. A public nested-bank crash/recovery/replay case refuses a
substituted default-root path claim and preserves unrelated staged code.

A shared resolver reads explicit local `agent-memory.bankRoot` and
`agent-memory.authorityBranch` settings. Bank helpers use its default or configured
root consistently; observed mutation scope pins the settings. Unsafe, ambiguous,
aliased or symlinked roots refuse. Changing configuration does not migrate a bank
or reinterpret an outstanding recovery journal. With no local location setting,
the unique Git-visible typed router locates the same bank on an existing checkout
or a fresh clone with empty private state. Location discovery does not adopt its
project identity; explicit binding remains required. Ignored alternate banks,
unknown/damaged recognized routers, competing locations or an incomplete/bounded
inventory refuse initialization rather than manufacture absence. Working, indexed
and current-commit routers are cross-checked so deletion, damage or reclassification
of a working/staged file cannot hide existing ownership. Git object readers are
bounded and run no filters. Discovery does not restore source bytes. Effective
resolved settings remain pinned across writes and recovery. Listed unsafe labels
are withheld before printing the directory prefix, branch or selected remote name.

`sharing_boundary.py --repo` inspects canonical source safety, Git index modes,
ignore rules and the selected remote/default-branch observation without writes
or networking. It names only eligible Markdown and reports blockers for ignored
or untracked memory, tracked excluded artifacts and unresolved or unsafe state.
Index membership does not prove current bytes are committed. Local remote HEAD
is not a freshness receipt. The configured/default authority must be verified
again at actual exchange, without feature-branch ancestry or branch-rule bypass.

Private locks, transaction markers, registries, journals, backups and projections
remain outside the admitted sharing delta. A clean boundary is not semantic
admission, publication or authentication. Publication, refresh, conflict handling
and retries are distinct reviewed work; this inspection does not implement them.
