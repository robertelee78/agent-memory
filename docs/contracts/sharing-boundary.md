# One configured repository bank and a memory-only Git boundary

The [portable sharing guide](../../skills/agent-memory/sharing/GUIDE.md) binds
canonical Markdown to one repository and keeps AWA's work evidence and Vox's
coordination separate. Local use requires neither service. There is no personal
memory namespace or cross-repository exchange.

A shared resolver reads explicit local `agent-memory.bankRoot` and
`agent-memory.authorityBranch` settings. Bank helpers use its default or configured
root consistently; observed mutation scope pins the settings. Unsafe, ambiguous,
aliased or symlinked roots refuse. Changing configuration does not migrate a bank
or reinterpret an outstanding recovery journal. A custom location must be
explicitly configured in another clone; a lost setting is not evidence of absence.

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
