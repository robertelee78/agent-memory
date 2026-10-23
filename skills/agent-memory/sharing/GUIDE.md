---
kind: domain
---

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


When obtaining fresh remote observations, reading back a lost response,
recovering from cursor loss or planning deletion-aware merges, a session MUST
read [recovery/GUIDE.md](recovery/GUIDE.md).
These recovery primitives do not themselves publish or refresh canonical memory.

A session MUST follow [publication/GUIDE.md](publication/GUIDE.md) at a natural
boundary to prepare and land admitted memory-only changes through actual
repository rules. Publication does not refresh local memory or feature code.

A session MUST follow [refresh/GUIDE.md](refresh/GUIDE.md) at a natural boundary
to reconcile landed memory with its unpublished local learning, without changing
feature code, the original index or branch history. It MUST read relevant imports
with their actual provenance and applicability before using them as current.

When local and landed memory are to be treated as compatible, a session MUST
first follow [semantic/GUIDE.md](semantic/GUIDE.md), including textually clean cross-file
claims, duplicates, source conditions and detecting-developer clarification.

## Domain loading and routes

A session MUST load this domain before its selected operation leaf and MUST NOT
preload sibling leaves for completeness. It MUST select the next operation from
actual observed state. The [shared model](../model/GUIDE.md) MUST load only when
a semantic question remains unresolved by this domain and operation.

A session MUST follow this domain before configuring the repository bank/default
branch or inspecting eligible memory-only Git paths; the [boundary operation](boundary/GUIDE.md) supplies that procedure.

A session MUST follow [sharing/semantic/GUIDE.md](../sharing/semantic/GUIDE.md)
when combining local and landed learning, to preserve source conditions and
origins, isolate disputed updates and review meaning across files.

When recovering a lost sharing response or reconstructing a merge baseline,
a session MUST first read [sharing/recovery/GUIDE.md](../sharing/recovery/GUIDE.md).

A session MUST follow [sharing/publication/GUIDE.md](../sharing/publication/GUIDE.md)
at natural turn boundaries to automatically prepare and land admitted memory
independently of feature code, respecting actual repository review/check rules.

A session MUST follow [sharing/refresh/GUIDE.md](../sharing/refresh/GUIDE.md) at
natural turn boundaries to import newly landed memory, preserve unpublished
learning and read relevant imports with their conditions and provenance.

A local edit, branch commit or successful validation MUST NOT be described as
landed shared memory. Other sessions can rely on a change only after the
repository's publication rules have landed it on `main` and they have refreshed
that memory. Memory-only publication and turn-boundary refresh are implemented
by the linked operation routes; the offline file workflow remains local.
While offline or exchange is unavailable, a session
MUST retain local work and report sharing as pending when freshness matters.

| Intent | Operation |
| --- | --- |
| Inspect the memory-only Git boundary | [boundary](boundary/GUIDE.md) |
| Review shared meaning | [sharing/semantic](../sharing/semantic/GUIDE.md) |
| Recover Git exchange/baselines | [sharing/recovery](../sharing/recovery/GUIDE.md) |
| Publish memory independently | [sharing/publication](../sharing/publication/GUIDE.md) |
| Refresh landed memory | [sharing/refresh](../sharing/refresh/GUIDE.md) |
