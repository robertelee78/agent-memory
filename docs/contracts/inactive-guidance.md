# Safe inactive representations and derived input

The portable [inactive guide](../../skills/agent-memory/inactive/GUIDE.md)
defines the representation boundary for superseded, revoked, expired and
deleted memory. Compaction is a pure format operation on an already inactive
record. It drops the original body and optional admission fields while keeping
required conditions, evidence, original provenance and a visible safe state
reason/replacement. Unsafe retained metadata refuses. Terminal deletion keeps
only the exact existing content-free tombstone format.

The public invalidation helper resolves the actual project and reads the whole
canonical bank. Derived input never supplies output prose or hashes: only
canonical active ID/version/path references remain. Deleted, absent, inactive
and wrong-version references are discarded. Stable identity/version alone
cannot authorize a cached body. The complete bank and selected identity are
rechecked before returning a private content-free audited result.

This does not choose or persist lifecycle changes, complete current recall,
purge Git history, invalidate third-party caches, or implement governed
forgetting/consolidation. Consumers must apply full current-selection and
security/admission checks, reread canonical text, and revalidate under their
observed write lock. Later operations must remove every owned unsafe residual
copy and cannot rebuild content from older projection input.
