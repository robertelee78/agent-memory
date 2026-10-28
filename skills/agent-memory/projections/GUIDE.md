---
kind: operation
domain: maintain
---

# Build and validate disposable canonical references

## Use when

A session SHOULD use this operation to build or check optional references to current canonical memory. It MUST NOT use a projection as the only durable copy of a lesson or as independent proof of applicability.

## Required context

A session using this leaf MUST first load and follow the [maintain domain](../maintain/GUIDE.md). It MUST open the [shared model](../model/GUIDE.md) only when an unresolved semantic question requires it.

The Markdown bank MUST hold the complete durable learning. A projection MAY
speed lookup but MUST NOT supply an independent record body, admit learning,
override lifecycle/applicability, or become the sole copy of memory. A session
MUST discard/rebuild stale projections instead of replaying them into the bank.
No SQLite store or optional service is required or enabled by this helper.

## Inputs and preconditions

The request contains exactly `project_id` and `records`; each record supplies
`id` and positive `version`. The [invalidation guard](../inactive/GUIDE.md)
accepts at most 64 references in a 64 KiB request. Old cached fields MAY appear
on input, but none is forwarded. The selected project MUST resolve before
reading canonical records. Deleted, absent, inactive or wrong-version records
MUST be dropped using current bank authority.

## Observe

A session MUST resolve the actual selected project and current canonical record IDs/versions before building. It MUST inspect any old projection as derived evidence and retain the bank as the source for current state.

## Plan

The session MUST plan a disposable reference object and distinguish any private audit state from canonical memory changes. It MUST NOT copy canonical lesson bodies into a second authority or require a cache for ordinary file-based resume.

## Apply

With `SKILL_DIR` resolved to this package:

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/projection.py" build --repo /path/to/repo \
  --input /private/path/references.json
```

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/projection.py" check --repo /path/to/repo \
  --input /private/path/projection.json
```

## Read back

The result's `projection` contains exactly `schema`, `project_id`, complete
`source_hashes` and canonical `records` with only ID/version/path. It includes
no independent body, title, trust label, remembered instruction or admission
decision. The schema is `agent-memory/reference-projection-v1`. A projection
is bounded to 2 MiB and 8192 source files; a larger bank MUST keep using its
canonical files or a separately reviewed bounded projection design.

The helper rechecks the complete bank and selected identity and records a
private content-free audit. It returns JSON; it does not create a cache file
or modify canonical memory. A session MAY retain just `projection` in a private
disposable file. It MUST NOT publish runtime projections as project memory.

The input MUST be the exact projection object, not its build-result envelope.
Check resolves the actual project, compares every canonical source path/hash,
revalidates exact active references, and rechecks identity and bank state.
Any changed/removed/added source, substituted reference, unsafe scope or failed
audit MUST refuse. A refusal MUST NOT echo stale hashes or payload that might
describe a now-deleted lesson. The caller MUST discard/regenerate its owned
cache and MUST NOT use a failed check as partial authorization.

## Verify

`valid: true` means those canonical references match the observed bank bytes.
It does not verify admission, source truth, current applicability, validity
time, trust/type, visibility, active-superseder conditions or publication.
Before actual recall a session MUST apply every current-selection and security
gate and read fresh canonical text. A later bank change invalidates the check;
a governed writer MUST revalidate under the [observed write lock](../writes/GUIDE.md).

The projection is repository-bound but contains no machine path or selected
developer namespace. Another authorized clone/developer MAY check it after
explicitly resolving the same project and identical bank bytes; provenance in
the canonical records MUST remain unchanged. Unrelated code edits do not make
a bank-byte observation proof of code behavior.

## Outcomes

Exit 0 means a built projection or `valid: true` check. Exit 1 means invalid,
stale or unauthorized input/state; canonical memory remains unchanged. Private
audit writes are reported separately. Removing a projection MUST NOT require
event replay, a database or recovery of memory from its hashes.

## Failure handling

If a projection is stale, malformed or bound elsewhere, the session MUST preserve useful canonical memory and rebuild from a fresh actual observation. It MUST NOT treat a failed check as current or silently widen project scope.

## Diagnostic fallback

If the helper cannot validate a derived object, the session MAY discard only known disposable projection state and resume from the bounded canonical router. It MUST preserve unknown transaction evidence and use explicit recovery for an interrupted bank rather than delete its marker.
