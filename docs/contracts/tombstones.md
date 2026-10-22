# Content-free terminal deletion markers

A tombstone prevents a deleted record ID from returning through an old projection
or consolidation output. It MUST retain exactly five fields: `id`, `scope`,
`deleted-at`, `deletion-authority` and `reason-class`. Its Markdown body MUST be
empty. The bank's versioned file contract defines this format; no extra schema,
source, status, evidence, title, original path, content or fingerprint field is
allowed inside the tombstone.

`id` MUST be the existing random UUIDv4 of the deleted unit, not a new deletion ID
or a value derived from its content. A generated source/projection MUST preserve
that ID; reminting IDs on rebuild defeats deletion. A prose section needing
independent deletion must have its own stable unit ID before another artifact
can reproduce it. A projection without source IDs MUST fail rather than infer
identity from deleted text or a hash.

`scope` MUST be the canonical project UUID. It deliberately retains no branch
name, component path, personal name or other descriptive prose. Deletion is
terminal for this record within the project, across its former conditions. A
correction or branch-specific loss of applicability uses scoped supersession or
pruning instead of pretending a terminal deletion leaves the same ID active in
another branch. Distinct retained knowledge keeps a distinct source ID.

`deletion-authority` MUST be a stable random developer UUID resolved by the
governed caller, not a display name or email. This is provenance, not proof of
identity or permission. `deleted-at` uses the shared precise timestamp parser.
`reason-class` is one of `user-request`, `incorrect`, `obsolete`, `sensitive` or
`retention`; it MUST NOT become a free-text explanation containing the erased
claim. The five-field allowlist cannot authenticate a claimed authority or prove
that a caller generated rather than fabricated a UUID.

## Format and placement

```sh
python3 -I scripts/tombstones.py render --input /tmp/deletion-metadata.json
python3 -I scripts/tombstones.py validate --file /path/to/tombstone.md
```

Both commands write nothing. Render returns the proposed bytes and canonical
relative path `history/tombstones/<record-id>.md`. The governed deletion caller
MUST persist the marker in project history whenever another artifact could
recreate the deleted guidance, remove its former source/active route, and update
the router through the observed multi-file recovery protocol. Directly routed
tombstones use repository scope, deleted lifecycle and a rebuild read condition.
The tombstone's project UUID must match the containing bank. The strict tombstone
directory contains UUID marker files only; a bounded history index belongs
outside that directory. Losing a disposable index must not lose this history.
The guard MUST inspect every physical occupant, including non-Markdown and
hidden runtime names. Extra files, subdirectories, wrong directory types and
case/Unicode spelling aliases MUST fail closed. It MUST repeat that namespace
check before returning survivors; a skipped file is not absent deletion history.

These helpers establish the format and rebuild guard. They do not perform the
full governed forget operation, purge existing caches/backups, erase Git history,
or authorize a mutation. Those operations have separate evidence and retention
contracts. Raw file access remains governed by repository/OS permissions.

## Rebuild guard

```sh
python3 -I scripts/tombstones.py filter --repo /path/to/repository \
  --input /tmp/source-projection.json
```

Input has `project_id` and a `records` list. Each source record MUST have a unique
stable random `id`; the caller's other fields, including conditions, evidence and
provenance, remain intact for surviving records. The guard reads canonical routed
history, suppresses every terminally deleted ID, and checks the bank again before
returning survivors plus source revision hashes. It does not materialize or
rewrite the input projection. Materialization callers MUST consume the guarded
result, and their erasure operation must purge the obsolete copies they own.
Changing a cached record's conditions does not evade terminal deletion.

Missing IDs, duplicate IDs, another project, invalid/unreachable history, changed
history, a lock or an interruption marker MUST stop rebuilding. The guard MUST
NOT treat unavailable history as proof that no deletion occurred. Rejected input
and errors MUST NOT echo deleted bodies or unrecognized sensitive fields.

This is only the terminal-deletion stage. It does not establish currentness,
trust, authorization or evidence quality for surviving candidates; normal
admission and selection gates still apply. Shared refresh/publication must first
make the applicable landed history available. An old branch without newer landed
tombstones cannot claim cross-session freshness on the strength of this local
check.
