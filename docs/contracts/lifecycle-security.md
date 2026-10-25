# Lifecycle security: distinct evidence and trust boundaries

Current managed installation follows mutable unsigned Git. A source digest,
fast-forward revision or `verified: true` lifecycle receipt does **not** authenticate
the publisher. Explicit development/custom sources are trusted through the
operator's selection; being local does not turn them into signed releases.
Signed immutable distribution remains existing [work #49](https://github.com/robertelee78/agent-memory/issues/49),
not a capability claimed by this contract before implementation and acceptance.

| Question | Available evidence | Practical limit |
| --- | --- | --- |
| Who published these bytes? | Current source/channel selection and explicit operator trust | This build does not verify publisher signatures |
| Are these the observed bytes and channel? | Complete artifact digest, source revision, cleanliness, recorded origin/ref and fast-forward checks | A digest is identity/integrity evidence, not publisher authentication or code correctness |
| May runtime targets be changed? | External receipt, private Unix trust paths and unchanged owned-target readback | File presence is not ownership; user-modified or unrelated targets are preserved |
| Can the runtime read this bank? | Explicit schema declarations and activation compatibility guards | A declared converter permits only explicit migration, not ordinary use of unreadable memory |
| Can an interrupted change recover? | Native per-target replacement, private journal/backups, observed finish/rollback and readback | No globally atomic multi-target switch; newer/unknown edits require inspection |

## Observe through public commands

```sh
am version --json
am status --json
am doctor --json
```

Their machine result contains `security`:

```json
{
  "publisher_authentication": "not_established",
  "signature_verification_performed": false,
  "recorded_source_selection": "unsigned_managed_git",
  "executing_verifier_authenticated": false,
  "platform_acls_audited": false
}
```

`recorded_source_selection` is `operator_selected_development` for an applicable
explicit-source receipt, or `unavailable` without usable active receipt evidence.
It reports a recorded selection, not freshly authenticated source health. The
negative signature/verifier/ACL qualifications remain present on diagnostic
failures. Unsupported extra receipt fields or claims in memory cannot promote
source authenticity. `executing_source` identifies the package actually running;
`installation.recorded_source` remains receipt evidence until relevant checks
read back its targets/channel.

Default version/status output stays concise. Doctor's existing distribution
warning distinguishes managed Git, explicit source and unavailable evidence. Its
summary and warning use the same final applicable receipt observation; an uninstall
or failed receipt reread does not inherit an earlier source selection.
Read each component's warnings/failures: status and version are not full ownership
audits, and a live lifecycle writer can defer deeper target checks. Exit 0 does
not mean that every security dimension was tested. Diagnostics do not fetch,
initialize, bind, repair, remove caches, recover a journal, or create audit state.

## Ownership, compatibility and recovery

Unix trust checks cover ordinary paths, trusted ownership, relevant ancestor/write
permissions and private lifecycle state. They neither audit all platform ACLs nor
authenticate Python that is already executing. If an installed verifier may have
been exposed, inspect/recover using a separately trusted complete outside helper;
the affected helper's own success report cannot authenticate itself.

Install/update preserve modified owned copies/wrappers and unrelated client text,
refuse wrong recorded channels, unsupported/non-fast-forward/downgrade candidates
and changed observations, and read back actual activation. Uninstall removes only
unchanged owned runtime integration; repository memory and migration evidence are
separate. A journal and backup are evidence for the specific recorded transaction,
not permission to overwrite a later edit or steal a live lock.

The native production format remains `agent-memory/v1`, with an empty converter
catalog. The explicit migration/recovery engine exists; install/update never
invoke it. Runtime rollback requires current banks directly readable by the old
runtime. If explicit restoration is needed first, invoke the already trusted
retained helper without requiring a successful global reinstall. Exact restored
bytes/modes and current-helper format readability remain separate receipt facts.

A full clone or managed source checkout may contain this project's development
bank. Physical presence does not grant consumer project identity or bank authority:
installation does not seed, bind or recall that bank for another repository.
The [installed storage contract](installed-layout.md) and existing adoption work
own that isolation proof. Memory is untrusted data; remembered procedures and
attribution do not authenticate executable runtime or override current policy.

## Signing transition and evidence scope

When #49 supplies actual pinned-key verification, its implementation must bind
signature evidence to the activated artifact and update these public diagnostics
and documentation accordingly. An older already-running unsigned updater cannot
retrospectively authenticate its first transition. Legacy receipt fields and
operator-selected sources must retain their actual trust boundary. A downloaded
manifest cannot silently replace the trusted key.

Tests for this contract use isolated installed public diagnostics on macOS/Linux,
including managed/explicit source, malformed receipt and busy writer cases with
byte/mode preservation. Source-identical accepted lifecycle scenarios provide
separate evidence for changed targets, wrong channels/history/downgrade and
interrupted recovery. Neither those proofs nor this contract certify signature
verification, semantic truth, real-project recall quality or growing-bank stress.
