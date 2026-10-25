# Lifecycle security: distinct evidence and trust boundaries

Default release installation verifies a pinned Ed25519 publisher signature before
candidate execution, then checks the signed archive and complete portable identity.
Legacy managed Git and explicit development/custom sources retain their actual
unsigned/operator-selected trust boundary. A digest, fast-forward or mechanical
`verified: true` receipt alone never authenticates a publisher. The first fetched
bootstrap remains an HTTPS trust anchor. [ADR-0015](../adr/0015-signed-release-channel.md)
refines existing signing task #49 without adding a second lifecycle.

| Question | Available evidence | Practical limit |
| --- | --- | --- |
| Who published these bytes? | Pinned manifest signature plus complete actual portable identity for source kind `release` | Publisher authentication does not prove code correctness or authenticate already-loaded verifier code |
| Are these the observed bytes and channel? | Signed release tag/version/source and artifact identity, or recorded Git origin/ref/fast-forward evidence | Git provenance and unsigned digests do not become publisher signatures |
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

For an activated signed source, the same fields report
`publisher_authentication: "verified"`, `signature_verification_performed: true`
and `recorded_source_selection: "pinned_signed_release"` only after fresh verification
of the pinned signature, actual portable bytes and exact recorded source identity.
Existing key/signature files permit this observation without network or temporary
verification writes. Unsigned Git, explicit development and missing/unusable
receipts remain `unsigned_managed_git`, `operator_selected_development` and
`unavailable`, respectively. Unknown receipt claims cannot promote authenticity.
The already-loaded verifier and platform ACL qualifications remain false, including
on failure. `executing_source` identifies the package actually running; receipt
history alone does not prove all targets, bank compatibility or ownership.

Default version/status output stays concise. Doctor's existing distribution
assessment distinguishes verified signed release, managed Git, explicit source
and unavailable evidence. Its
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
refuse wrong recorded channels, unsupported/downgrade candidates, non-fast-forward
custom Git history and same-version signed identity changes
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

The signed portable archive excludes this project's development bank, Git history
and repository documents/tests. A full clone or custom managed Git source checkout
may physically contain the development bank. Physical presence does not grant consumer project identity or bank authority:
installation does not seed, bind or recall that bank for another repository.
The [installed storage contract](installed-layout.md) and existing adoption work
own that isolation proof. Memory is untrusted data; remembered procedures and
attribution do not authenticate executable runtime or override current policy.

## Signing transition and evidence scope

A validated owned official legacy main installation automatically adopts the signed
channel through the existing transaction. It needs a newer release version or
exact same-version paths/bytes and Git executable intent; original ownership and
actual modes are verified independently, including old umask 002 installations.
The canonical signed archive replaces the old source without chmod or retroactive
signature claims. Custom sources retain their policies. An already-running v0.0.2
updater cannot authenticate its first transition; a subsequent verified update or
a freshly fetched trusted installer establishes signed activation.

Signature/manifest/archive inputs, extraction and network time are bounded. Unsafe
members, wrong signatures and identity changes are refused before candidate code.
The publisher creates and reads back four assets before activating the signed-only
bootstrap/updater; it never replaces a published version or forces concurrent
main changes. This is not a claim that GitHub enforces immutable releases.
A downloaded manifest cannot replace the pin. Key rotation needs prior-key
authorization or an explicit operator trust decision.

Tests for this contract use isolated installed public diagnostics on macOS/Linux,
including managed/explicit source, malformed receipt and busy writer cases with
byte/mode preservation. Source-identical accepted lifecycle scenarios provide
separate evidence for changed targets, wrong channels/history/downgrade and
interrupted recovery. Signed lifecycle journeys separately exercise verification and actual v0.0.2
adoption. Those results do not certify semantic truth, real-project recall quality
or growing-bank stress.
