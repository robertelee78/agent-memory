---
kind: operation
domain: govern
---

# Represent inactive guidance and filter stale derived references

## Use when

A session SHOULD use this operation to compact an already inactive representation or remove invalidated cached references before rebuilding. To deactivate or erase active canonical learning, it MUST select governed correction/forgetting instead.

## Required context

A session using this leaf MUST first load and follow the [govern domain](../govern/GUIDE.md). It MUST open the [shared model](../model/GUIDE.md) only when an unresolved semantic question requires it.

A session MUST exclude superseded, revoked, expired and deleted lessons from
current guidance. An old cached body, search hit, consolidation candidate or
historical event MUST NOT reactivate one. Confidence, recent access and a new
landing order do not change lifecycle authority. The [shared model](../model/GUIDE.md)
defines the independent current-selection gates.

## Inputs and preconditions

Revocation, supersession and expiry MAY retain a concise safe explanation,
required original provenance, evidence, applicability and replacement links.
A session MUST remove the old assertion from current context as well; changing
the standalone record does not correct a copied prose summary by itself.

Input contains exactly `project_id` and `records`; each of at most 64 records
MUST contain a stable `id` and positive `version`. A cached record MAY also
contain old fields such as a body, title or hash, but none of those payload
fields is forwarded. The whole input is bounded to 64 KiB.

## Observe

Before using a cached/consolidated record reference, a session MUST resolve
the actual project and consult the current canonical bank:

## Plan

A session MUST distinguish rendering a safe inactive representation from applying a lifecycle change or rebuilding a view. It MUST retain the actual current bank and tombstone evidence; this helper is not permission to overwrite them.

## Apply

The pure format helper compacts an **already inactive** standalone record:

```sh
python3 -I "$SKILL_DIR/scripts/inactive_records.py" compact \
  --file /private/path/inactive-record.md
```

```sh
python3 -I "$SKILL_DIR/scripts/inactive_records.py" filter --repo /path/to/repo \
  --input /private/path/derived-records.json
```

## Read back

It returns a new Markdown representation without writing the bank or deciding
a lifecycle change. It drops the original body and optional admission fields,
keeps the schema-required conditions/evidence/provenance plus visible state
reason and replacement links, and uses a fixed non-guidance body. Retained
metadata MUST pass safety checks and the representation MUST fit 16 KiB. A
session MUST NOT silently discard
distinct conditions/evidence or manufacture substitute attribution. The
governed correction/forgetting operation MUST resolve that problem or use an
explicit safe deletion when authorized.

Compaction of an exact terminal tombstone returns only its existing five safe
fields with an empty body. A session MUST NOT add titles, original bodies,
source excerpts, free-form reasons, source hashes or reversible/low-entropy
content hashes to a tombstone. Deletion MUST remove unsafe copies from every
owned derived artifact; Git history is an explicit separate retention boundary,
not a claim that old repository objects have been purged.

The helper resolves the selected project, validates the complete bank,
checks repository-wide terminal tombstones, reads the exact canonical versions
and rechecks the complete observation. Deleted, absent, wrong-version and
non-active records are discarded. Remaining results contain only canonical
`id`, `version` and `path` references. They contain no cached body or content
hash. A private content-free audit records the mediated invalidation check.
Invalid scope, broken/changed bank, unresolved identity or failed audit MUST
refuse without changing canonical files.

## Verify

This is an erasure/invalidation guard, **not full recall**. A remaining reference
MUST still pass source/admission qualification, authorized applicability,
validity time, trust/type, visibility, active-superseder and
[security](../security/GUIDE.md) checks before its canonical text can enter
current guidance. A session MUST read that text from its fresh canonical source
rather than reuse a cached payload. A later bank change invalidates the check;
consumers MUST repeat it before a rebuild/consolidation write and recheck their
observations under the bank lock when applying a proposal.

The older `tombstones.py filter` remains a terminal-deletion-only format guard;
a session MUST NOT treat it as lifecycle-current recall. Neither helper owns
or invalidates arbitrary third-party caches, authenticates a developer, admits
learning or publishes memory. A session MUST follow
[governance](../governance/GUIDE.md) for actual lifecycle decisions and
[consolidation](../consolidation/GUIDE.md) for meaning-preserving maintenance.
Those operations MUST use these canonical safeguards
for each derived input and invalidate every view/cache/index they own.

## Outcomes

Exit 0 means a safe representation or a completed invalidation check. Exit 1
means bounded input, metadata, scope, audit or source state failed validation;
the session MUST preserve original state and surface the failure. A session
MUST NOT infer completed erasure or current guidance from format success.

## Failure handling

If canonical invalidation cannot be established, the session MUST retain the input as non-current evidence and obtain a fresh actual bank observation. It MUST NOT restore an erased body from a cache or treat a failed filter as an empty safe result.

## Diagnostic fallback

If the normal helper cannot establish a result, a session MAY inspect the current router, canonical record headers and safe tombstone metadata with ordinary file tools. It MUST preserve unknown locks/markers and MUST NOT delete recovery evidence or recreate a terminally deleted ID.
