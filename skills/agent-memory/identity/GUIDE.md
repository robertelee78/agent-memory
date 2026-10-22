# Resolve and bind repository identity

A session MUST use these common helpers before mediated memory writes. Clients
MUST NOT derive project IDs from directory names, Git credentials, harnesses or
their own algorithms. Local registries hold identity metadata, never personal
memory. Developer IDs supply provenance; repository and OS permissions remain
the access boundary. Raw Markdown reads and manual edits do not require this
setup and do not create mediated audit evidence.

With `SKILL_DIR` resolved to this portable package:

```sh
python3 -I "$SKILL_DIR/scripts/scope_binding.py" status --repo /path/to/repo
```

Status is read-only. It shows the selected developer ID, project ID, attribution
observations, common Git directory, sanitized authority evidence, registry
bindings and unresolved issues. Harness and session are separate context:
`--harness custom --session session-1`. Unknown aliases remain visible; an
explicitly selected stable ID is not silently replaced by a changed account.
Conflicting known aliases MUST be clarified by the detecting developer. A
session MAY supply known observations using `--observe kind=value`; it MUST NOT
describe those observations or IDs as authenticated identity.

## Developer setup

Selecting or creating a developer identity MUST be explicit. A session MUST
preview and review the concrete proposal before applying it:

```sh
python3 -I "$SKILL_DIR/scripts/developer_identity.py" preview --create \
  --label 'Developer label' > /private/path/developer.json
python3 -I "$SKILL_DIR/scripts/developer_identity.py" apply \
  --proposal /private/path/developer.json
```

On another machine, `preview --adopt UUID` explicitly adopts the same stable
developer ID. A different developer MUST retain a distinct ID. `preview
--select UUID` explicitly selects an already registered ID. `--alias
git-author-email=value` or another supported alias adds evidence; it MUST NOT
create a different memory namespace or imply access control. Selection and
aliases do not rewrite existing record origins.

## Bind, link, rebind or fork

A session MUST choose the intended operation based on the observed evidence:

| Action | Intended decision |
| --- | --- |
| `create` | Register a new empty checkout and allocate a random project UUID before initialization |
| `bind` | Adopt the UUID of an existing, previously unregistered bank |
| `link` | Explicitly join an independent checkout to an already registered matching bank UUID |
| `rebind` | Keep the UUID and explicitly change authority evidence, retaining historical remote aliases |
| `fork` | Allocate a new UUID for an existing copied bank or checkout |

```sh
python3 -I "$SKILL_DIR/scripts/scope_binding.py" preview --repo /path/to/repo \
  --action bind --reason 'Adopt this existing project bank' > /private/path/bind.json
python3 -I "$SKILL_DIR/scripts/scope_binding.py" apply --proposal /private/path/bind.json
```

Preview writes nothing. A session MUST inspect the registry decision, complete
bank proposal and `changed_memory_paths`. Ambiguous remotes require an explicit
`--authority NAME` or repair; this is not permission to change repository policy.
The common algorithm examines the real common Git directory so linked worktrees
share an identity. Independent clones require explicit link/bind/fork decisions.
Moves on the same filesystem retain the location identity. Without a remote,
the UUID and location binding remain valid. Sanitized `repo://` identities are
evidence identifiers, not transport URLs.

Apply checks the developer selection and preview read set, updates the registry,
then uses the observed multi-file protocol for necessary bank changes. The
registry and bank are separate stores; a session MUST NOT claim cross-store
atomicity. `verified: true` and `resolved: true` require readback of both stores.
Forking changes every typed record/index/event project namespace and terminal
tombstone scope while preserving record IDs, versions, conditions, evidence,
source origins, deletion authority and bodies. Context prose and unrelated code,
index, HEAD and feature ancestry are preserved. Old remote aliases remain in
registry history. This operation does not publish, fetch, merge or authenticate.

### Empty-bank handoff

`create` deliberately returns `registry_verified: true`, `verified: false` and
`initialization_pending: true` until a useful bank exists. Its
`initialization_context` provides resolved IDs for the initializer; it authorizes
only that initial empty-bank operation. The shared `require --initialize` path
also exposes this context without creating files. Ordinary writers MUST refuse
the pending state. Once initialization is complete, ordinary resolution succeeds
and `--initialize` refuses the existing bank. The full evidence-based initializer
is a separate operation; `create` MUST NOT manufacture project requirements.

### Interruption and retry

A session MUST retain the original preview. Before bank-changing binding begins,
an exact private `binding.json` recovery bundle is saved under the local state
root. It includes bank content and MUST NOT be committed or published. After a
verified transaction outcome, it is removed; content-free receipts remain.
An interruption after registry update can leave identity pending. A session MUST
report that state and retry the same bundle, rather than guess a new identity.

A visible bank lock requires its explicit preview/readback recovery first:

```sh
python3 -I "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo > /private/path/unlock.json
python3 -I "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo \
  --proposal /private/path/unlock.json
python3 -I "$SKILL_DIR/scripts/scope_binding.py" apply --proposal /private/path/bind.json
```

Live, foreign or unknown locks MUST remain for inspection; age is not ownership.
The binding retry finishes its own matching interrupted transaction. Ordinary
transaction recovery MUST NOT finish an identity change outside its original
binding decision. Changed developer selection, authority, bank bytes, unknown
markers or a later binding decision MUST stop automatic replay and surface the
problem to the detecting developer. A session MUST preserve the original
provenance rather than reattribute old work to a newly selected developer.

The common single-file and multi-file write helpers pin resolved scope when a
proposal is prepared, then recheck it before applying and under the bank lock.
An unresolved proposal can be inspected but cannot be applied; finish setup and
preview again. Explicit recovery can be performed by the current resolved
developer without rewriting the original records' origins. Lock cleanup changes
runtime state only and remains available before bank resolution. No global
account switch, force push, repository protection change, personal memory or
cross-project federation is part of binding.

Local config defaults to `XDG_CONFIG_HOME/agent-memory`; local state defaults to
`XDG_STATE_HOME/agent-memory` (ordinary home defaults apply when unset).
`AGENT_MEMORY_CONFIG_HOME` and `AGENT_MEMORY_STATE_HOME` MAY select canonical,
absolute private roots. State MUST remain outside the repository. Registry
symlinks, corrupt data and unexpected external changes MUST be preserved for
explicit repair, not silently replaced. A session SHOULD keep normal operation
quiet while surfacing conflicts, failures and consequential identity corrections.
