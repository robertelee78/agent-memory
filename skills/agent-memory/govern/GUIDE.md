---
kind: domain
---

# Govern project memory

Correct or stop invalid guidance while preserving still-valid conditions, history and safe erasure. A consequential correction or unresolved conflict reaches the detecting developer.

A session MUST load this domain before its selected operation leaf and MUST NOT
preload sibling leaves for completeness. It MUST return to observed state before
selecting another operation. It MUST open the [shared model](../model/GUIDE.md)
only when a scope, authority, type, trust, lifecycle, validity or contradiction
question cannot be resolved by this domain and the selected operation.

## Standing rules

When performing correction, supersession, revocation, known expiry, policy
retention or explicit deletion, a session MUST follow
[governance/GUIDE.md](../governance/GUIDE.md).
It MUST automatically prune only with stronger evidence, preserve still-valid
older conditions, and leave ambiguous meaning with the detecting developer.
Age alone MUST NOT justify pruning; terminal deletion MUST leave safe tombstones.

A session MUST read [inactive/GUIDE.md](../inactive/GUIDE.md) when representing
supersession/revocation/deletion or using cached records for rebuild or
consolidation. Cached bodies MUST NOT override current canonical invalidation.

## Route by current intent

| Intent | Operation |
| --- | --- |
| Correct/prune/forget learning | [governance](../governance/GUIDE.md) |
| Inspect inactive guidance | [inactive](../inactive/GUIDE.md) |
