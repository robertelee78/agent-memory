# Observed writes and disposable reference projections

The existing common single/multi-file writers remain the mutation boundary:
exclusive bank lock before inventory through readback, observed-content checks,
atomic individual replacements, stable operation IDs and detectable multi-file
recovery. The portable [write guide](../../skills/agent-memory/writes/GUIDE.md)
teaches these public commands and preserves the distinction between a verified
earlier operation and a still-current result. No global transaction is claimed.

The [projection helper](../../skills/agent-memory/projections/GUIDE.md) builds
only canonical ID/version/path references plus a complete source path/hash map.
It requires actual project resolution, applies terminal/inactive invalidation,
and rereads identity and bank state. Cached prose, hashes, titles and authority
labels cannot become canonical reference content. A projection cannot hold the
sole admitted memory because it contains no independent body.

Check refuses changed/added/removed source files and altered/inactive references
without echoing stale data. A clean check is bank-byte freshness only; current
recall still requires admission, authority, applicability, validity, trust/type,
visibility and active-superseder checks. Consumers must recheck observations
under their write lock before mutation and discard/rebuild invalid owned caches.

The helper emits JSON and private content-free audits but does not own a cache
file, enable SQLite, replay history, authenticate developers or publish memory.
An optional future historical-event writer must use its stable event ID as the
idempotency key. It is not needed to reconstruct the current Markdown bank.
