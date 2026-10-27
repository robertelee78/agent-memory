---
kind: operation
domain: access
---

# Set up identity and bind the intended repository

## Use when

A session SHOULD use this operation when the common resolver reports missing/ambiguous developer or project binding, or an explicit link/rebind/fork is intended. An already resolved session MUST keep using the shared resolver rather than create a new identity per client or worktree.

## Required context

A session using this leaf MUST first load and follow the [access domain](../access/GUIDE.md). It MUST open the [shared model](../model/GUIDE.md) only when an unresolved semantic question requires it.

Common status, repository-location, attribution and read-set rules live in this domain. The session MUST use that observed state to choose the concrete setup/binding operation.

## Inputs and preconditions

Selecting or creating a developer identity MUST be explicit. A session MUST
preview and review the concrete proposal before applying it:

On another machine, `preview --adopt UUID` explicitly adopts the same stable
developer ID. A different developer MUST retain a distinct ID. `preview
--select UUID` explicitly selects an already registered ID. `--alias
git-author-email=value` or another supported alias adds evidence; it MUST NOT
create a different memory namespace or imply access control. Selection and
aliases do not rewrite existing record origins.

A session MUST choose the intended operation based on the observed evidence:

| Action | Intended decision |
| --- | --- |
| `create` | Register a new empty checkout and allocate a random project UUID before initialization |
| `bind` | Adopt the UUID of an existing, previously unregistered bank |
| `link` | Explicitly join an independent checkout to an already registered matching bank UUID |
| `rebind` | Keep the UUID and explicitly change authority evidence, retaining historical remote aliases |
| `fork` | Allocate a new UUID for an existing copied bank or checkout |

## Observe

A session MUST run the domain’s read-only status and inspect existing bank/registry/authority evidence before choosing an action. It MUST preserve a known UUID and clarify conflicting aliases rather than silently reminting identity.

## Plan

Preview writes nothing. A session MUST inspect the registry decision, complete
bank proposal and `changed_memory_paths`. Ambiguous remotes require an explicit
`--authority NAME` or repair; this is not permission to change repository policy.
The common algorithm examines the real common Git directory so linked worktrees
share an identity. Independent clones require explicit link/bind/fork decisions.
Moves on the same filesystem retain the location identity. Without a remote,
the UUID and location binding remain valid. Sanitized `repo://` identities are
evidence identifiers, not transport URLs.

## Apply

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/developer_identity.py" preview --create \
  --label 'Developer label' > /private/path/developer.json
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/developer_identity.py" apply \
  --proposal /private/path/developer.json
```

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/scope_binding.py" preview --repo /path/to/repo \
  --action bind --reason 'Adopt this existing project bank' > /private/path/bind.json
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/scope_binding.py" apply --proposal /private/path/bind.json
```

## Read back

Apply checks the developer selection and preview read set, updates the registry,
then uses the observed multi-file protocol for necessary bank changes. The
registry and bank are separate stores; a session MUST NOT claim cross-store
atomicity. `verified: true` and `resolved: true` require readback of both stores.
Forking changes every typed record/index/event project namespace and terminal
tombstone scope while preserving record IDs, versions, conditions, evidence,
source origins, deletion authority and bodies. Context prose and unrelated code,
index, HEAD and feature ancestry are preserved. Old remote aliases remain in
registry history. This operation does not publish, fetch, merge or authenticate.

## Verify

The session MUST verify the actual returned registry/bank readback, resolved identity and intended scope. It MUST preserve originating record identities/evidence and separate registry verification from completed useful-bank initialization.

## Outcomes

`create` deliberately returns `registry_verified: true`, `verified: false` and
`initialization_pending: true` until a useful bank exists. Its
`initialization_context` provides resolved IDs for the initializer; it authorizes
only that initial empty-bank operation. The shared `require --initialize` path
also exposes this context without creating files. Ordinary writers MUST refuse
the pending state. Once initialization is complete, ordinary resolution succeeds
and `--initialize` refuses the existing bank. The full evidence-based initializer
is a separate operation; `create` MUST NOT manufacture project requirements.

## Failure handling

A session MUST retain the original preview. Before bank-changing binding begins,
an exact private `binding.json` recovery bundle is saved under the local state
root. It includes bank content and MUST NOT be committed or published. After a
verified transaction outcome, it is removed; content-free receipts remain.
An interruption after registry update can leave identity pending. A session MUST
report that state and retry the same bundle, rather than guess a new identity.

A visible bank lock requires its explicit preview/readback recovery first:

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo > /private/path/unlock.json
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo \
  --proposal /private/path/unlock.json
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/scope_binding.py" apply --proposal /private/path/bind.json
```

Live, foreign or unknown locks MUST remain for inspection; age is not ownership.
The binding retry finishes its own matching interrupted transaction. Ordinary
transaction recovery MUST NOT finish an identity change outside its original
binding decision. Changed developer selection, authority, bank bytes, unknown
markers or a later binding decision MUST stop automatic replay and surface the
problem to the detecting developer. A session MUST preserve the original
provenance rather than reattribute old work to a newly selected developer.

## Diagnostic fallback

If ordinary setup cannot establish a verified binding, the session MAY inspect the actual registry, Git authority evidence and canonical router with read-only tools. It MUST retain the original proposal/recovery bundle and unresolved state, and MUST NOT force a new project/developer UUID or replace an occupied bank to bypass the refusal.
