# Stable developer attribution

Developer identity is a random stable UUIDv4 used for provenance. A developer
MAY explicitly adopt the same ID on two to five machines. A collaborator MUST
retain a distinct ID; harness and session identities MUST remain separate.
Developer IDs MUST NOT be treated as authentication, memory ACLs, personal
memory namespaces or personal preference stores. Repository and OS permissions
govern access to the shared project bank.

The [registry definition](../../skills/agent-memory/schemas/developers.json)
permits identity labels, declared aliases, the explicitly selected ID and
idempotent operation metadata only. It rejects memory/preferences fields and
harness/session aliases. Alias matches are evidence for clarification; they
MUST NOT silently select, create or replace a developer ID. Credentials,
tokens and learned project content MUST NOT be stored in this registry.

## Checkout-first setup

No installation, client, service, existing registry or project Memory Bank is
needed. These public commands are exercised by the isolated contract suite:

```sh
python3 -I scripts/developer_identity.py status
python3 -I scripts/developer_identity.py preview --create --label 'Developer A' > proposal.json
python3 -I scripts/developer_identity.py apply --proposal proposal.json
```

To reuse an existing stable ID, the agent MUST explicitly use `preview --adopt
UUID`. To select another ID already registered on this machine, it MUST use
`preview --select UUID`, then apply its reviewed proposal. An unknown ID MUST
NOT be selected implicitly. Repeated installation MUST reuse the existing
selected identity unless an explicit create/adopt/select decision says
otherwise; installer integration is verified separately in #46.

`--alias kind=value` adds explicitly declared metadata during create/adopt.
The supported kinds are os-user, git-author-name, git-author-email,
github-login and external. Adoption of an already registered ID preserves and
combines its existing labels and aliases. Selecting an ID cannot modify its
metadata. Labels and aliases MUST NOT become identity authority or learned
preferences.

`status` inspects Git author configuration when available and accepts explicit
`--observe kind=value` evidence. It reports unknown or conflicting aliases
without changing the selected ID or registry. A changed Git/hosting account,
OS alias or harness MUST NOT silently relabel attribution. `--harness` and
`--session` are reported as separate session context and are not persisted in
the developer registry. A session MUST retain its resolved developer ID for
in-flight learning; if the explicit default selection changes, its mediated
caller MUST resolve the change rather than silently relabel that learning.
Conflicting provenance MUST remain visible for clarification by the detecting
developer/session.

The portable equivalent is
`skills/agent-memory/scripts/developer_identity.py`. Both entrypoints use the
same implementation and schema. All output is JSON. Exit 0 means a status,
preview or verified operation; exit 1 means a refusal; exit 2 means invalid
command arguments. Read-only commands MUST NOT create config/runtime files.

## Registry, write and replay boundaries

The registry lives at
`${AGENT_MEMORY_CONFIG_HOME:-${XDG_CONFIG_HOME:-$HOME/.config}/agent-memory}/developers.json`.
These are local identity metadata, not a durable project-memory bank. The
initializer MUST NOT create or modify repository memory, Git history, client
activation, credentials or global Git configuration. Ordinary runtime removal
SHOULD preserve this stable attribution metadata unless explicit data removal
is requested.

A preview binds a random operation ID, explicit action/ID/metadata, exact
registry path and observed content hash. It writes nothing. Apply MUST reject
changed registry contents, a different config root, unsupported schemas,
unrecognized fields and symlink targets. It MUST hold an exclusive-create lock
from inventory through verified readback, recheck the observed hash before
replacement, use a same-directory atomic replacement and write a mode-0600
registry. Runtime lock/temp files MUST NOT enter any shared Memory Bank.

Repeated application of the same proposal MUST converge without creating a
second identity or operation. A reused operation ID with different content
MUST fail. If another explicit selection happened since the original
operation, replay MUST preserve the current selection and report both the
operation's developer ID and the current selected ID. A lost response MUST NOT
rotate or reselect an identity on retry. The selected ID is provenance and is
always reported with `identity_authenticated: false` by status.

Unknown/corrupt existing registry content MUST be preserved for explicit repair.
Refusals MUST NOT overwrite it. A successful durable mutation reports
`verified: true` only after exact content readback; an unverified result MUST
be inspected/replayed rather than assumed successful or repeated under a new
operation ID.

## Stale lock recovery

`recover` previews a bounded recovery proposal; `recover --proposal FILE`
applies that reviewed proposal. Recovery MUST prove the recorded same-host
process is gone. A live owner, foreign host, unknown lock shape or inability to
prove death MUST stop without removing the lock. Lock age alone is insufficient.

On POSIX systems, recovery also holds a nonblocking descriptor lock and checks
the file's inode and observed bytes before removing it. The open descriptor
prevents a competing recovery from removing a newly created writer's lock.
The result verifies removal of the original lock; a legitimate new writer MAY
already have acquired a replacement lock. Unsupported safe recovery MUST
refuse and require operator review. The filesystem MUST support reliable
exclusive creation, atomic rename and the stated locking semantics; these
checks do not promise coordination with non-cooperating manual writers.

The offline suite uses five isolated profiles, actual Git config changes,
competing initializer processes, idempotent replay, stale/external-change
proposals, local/foreign/live locks, descriptor-held recovery and copied packs
with spaces in their paths. Full installed lifecycle and project binding have
separate release tasks; this initializer does not require those tasks to have
already installed it.
