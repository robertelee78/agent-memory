# Install, update, and uninstall blueprint

Status: Design input for ADR-0007, which remains Proposed.

## Product boundary

Installation manages code, the portable skill, command wrappers, and bounded
client activation. It does not own project Memory Banks. Runtime lifecycle and
user-data lifecycle are separate authority domains.

The design inherits AWA's managed-checkout, preflight, backup, sentinel-block,
receipt, digest, and fast-forward update patterns. It strengthens transaction,
rollback, stale-lock, uninstall, and data-preservation behavior.

## Required command surface

```text
agent-memory install [--check|--dry-run] [--mode link|copy] [client targets...]
agent-memory uninstall [--check|--dry-run]
agent-memory update --check
agent-memory update
agent-memory version [--json]
agent-memory doctor [--json]
agent-memory status [--json]
```

Data deletion SHALL be a separate command or explicit `--purge-data` operation.
It SHALL NOT be an implicit uninstall option hidden behind an ordinary prompt.

All commands SHALL support deterministic machine-readable results. A command
that changes state SHALL return a receipt only after readback verification.

## Owned paths and receipt

The installer SHALL maintain one machine-readable receipt recording:

- receipt schema and installer version;
- installation ID and timestamp;
- source remote, requested ref, resolved revision, and artifact digest;
- managed checkout path or explicit development source;
- install mode and every enabled adapter;
- every path created, linked, copied, backed up, or sentinel-edited;
- the pre-change hash and installed hash of managed files or blocks;
- command-wrapper path;
- runtime state path and data roots, clearly distinguished;
- last successful verification result;
- rollback material and whether it is still available.

The authoritative receipt SHALL be
`${XDG_STATE_HOME:-$HOME/.local/state}/agent-memory/install/receipt.json`. Rotated
receipt backups and the transaction journal SHALL live under the same
installer-owned state directory, never only inside the managed code checkout.

An uninstall SHALL act only on paths and sentinel blocks whose current identity
still matches that receipt. Modified or unrelated content MUST be preserved and
reported for manual reconciliation.

## Installation transaction

Installation SHALL follow this sequence:

```text
resolve source and targets
  → inventory current filesystem
  → validate source artifact
  → compute exact delta
  → preflight every target
  → stage all files/links/blocks
  → create recoverable backups
  → atomically activate each staged target
  → read back every target
  → validate client-visible artifact
  → write verified receipt atomically
```

No target SHALL be changed before every target has passed preflight. Activation
SHOULD use same-filesystem atomic rename or symlink replacement per target.
Targets on different filesystems cannot form one atomic transaction, so the
installer MUST NOT claim global atomicity. If any target cannot activate or
verify, the installer SHALL restore the prior owned state and SHALL retain
enough journal information for `doctor` to finish or reverse the interrupted
transaction.

The installer MUST refuse to overwrite an unrelated directory, file, symlink,
command, or client instruction block. Copy mode and link mode MUST converge on
the same artifact digest.

## Client targets

The default target set SHOULD support:

- generic Agent Skills discovery;
- Codex skills and bounded `AGENTS.md` activation;
- Claude Code skills and bounded `CLAUDE.md`/`AGENTS.md` activation;
- OpenCode skills and bounded `AGENTS.md` or configured instructions;
- explicit custom target directories;
- a no-client-links mode for development and CI.

Activation text SHALL be a small sentinel-delimited shim. It SHOULD say when to
invoke the skill—for example, when a repository contains `memory-bank/MEMORY.md`
or when the user asks to initialize, resume, checkpoint, remember, correct, or
forget. It SHALL NOT duplicate the skill protocol in each client file.

Every enabled client MUST resolve to the same portable artifact revision and
digest. Client-specific adapters MAY differ, but memory semantics MUST NOT.

## Managed and development sources

The normal channel SHALL use one installer-owned checkout in an XDG-compatible
data location. A development install MAY point to an explicit local checkout,
but update SHALL NOT mutate that checkout and version output SHALL clearly mark
it as development and dirty when applicable.

The managed checkout SHALL record its expected remote and ref. The updater MUST
refuse an unexpected remote, unexpected branch/ref, dirty working tree,
non-fast-forward history, downgrade, invalid pack, or mismatched receipt.

## Update transaction

`update --check` SHALL be read-only. It SHALL report current and available
revision/version, channel/ref, cleanliness, compatibility or migration needs,
and whether client-visible content would change.

An update SHALL:

1. acquire a recoverable lifecycle lock containing owner process and start
   metadata;
2. validate the current receipt, checkout, remote, ref, and installed targets;
3. fetch without changing the active installation;
4. prove the candidate is allowed by channel and fast-forward policy;
5. stage and validate the complete candidate artifact;
6. run compatibility and migration preflight against durable data without
   mutating it;
7. stage all updated client targets and wrappers;
8. activate each target with an atomic per-target replacement and journal the
   completed target set;
9. read back and verify every target and the installed command;
10. atomically replace the receipt and retain a rollback point;
11. print restart guidance only for clients whose loaded artifact changed.

Failure before activation leaves the prior installation untouched. Failure
after activation SHALL trigger rollback or leave an explicit resumable journal
that `doctor` can repair without guessing.

Runtime update SHALL NOT migrate a project bank. Every runtime SHALL declare its
directly readable schema range and its bundled migrator source range. Update
SHALL refuse when a discovered bank fits neither range; a bank discovered later
SHALL remain read-only until an explicit supported migration. The separate
`migrate` operation SHALL preview affected banks, create recoverable backups,
execute the complete version-by-version chain, update one bank at a time, read
back the result, and record its receipt.

The ordinary downgrade guard does not block rollback to the exact runtime
recorded at the start of the current lifecycle transaction. After an explicit
bank migration, runtime rollback SHALL prove backward readability, run a
verified reverse chain, or restore the migration backup; otherwise it MUST stop.

The initial channel MAY use verified HTTPS and fast-forward Git history. A
signed immutable release channel SHOULD follow. The trust limitation of the
initial channel SHALL be visible in documentation and `doctor` output.

## Uninstall transaction

Ordinary uninstall SHALL:

1. load and validate the ownership receipt;
2. inventory all installed targets and compare their current identity/hash;
3. show the exact removal/restoration delta;
4. remove only unchanged paths owned by the receipt;
5. restore sentinel-managed client files without changing surrounding content;
6. preserve user modifications and report conflicts;
7. preserve project Memory Banks, indexes, logs, and backups;
8. read back target absence/restoration;
9. retain a minimal uninstall receipt and reinstall instructions.

The managed code checkout MAY be removed only after all active targets no longer
reference it. Backups SHOULD be retained by default with a bounded cleanup
command.

`--purge-data` SHALL first inventory each data root, classify project memory and
generated/runtime state, show exact paths and counts, and require explicit confirmation
for the resolved targets. It MUST NOT accept an unresolved broad path, symlink
escape, home directory, or filesystem root.

## `version`, `status`, and `doctor`

`version` SHALL report the semantic version, source revision, source path,
artifact digest, install mode, channel/ref, dirty qualification, and receipt
schema.

`status` SHALL be cheap and read-only. It SHOULD report:

- installation health and enabled adapters;
- resolved developer and current project identity;
- current bank path and format version;
- current/available update summary when already cached;
- optional index/federation presence and freshness;
- any incomplete lifecycle or memory transaction.

`doctor` SHALL perform the deeper audit:

- executable prerequisites and filesystem capabilities;
- receipt integrity and owned-path identity;
- source remote/ref/cleanliness and artifact digest;
- exactly one registered `SKILL.md` and a valid route graph;
- client links/copies and sentinel blocks;
- wrapper resolution and executable modes;
- data readability and schema compatibility;
- atomic-write, lock, and backup permissions;
- pending transaction recovery;
- projection freshness and full rebuildability from Markdown;
- update and migration preconditions.

Every failure SHALL name the affected path or component, explain the violated
contract, and give one bounded repair. Warnings and failures SHALL be distinct.

## Verification matrix

Lifecycle tests SHALL invoke the real installer and CLI in temporary home, XDG,
Codex, Claude, and OpenCode roots. They SHALL cover:

- fresh install and exact unchanged reinstall;
- link, copy, custom-target, and no-client modes;
- paths with spaces and missing optional clients;
- unrelated target refusal and managed-target backup;
- user edits inside and outside sentinel blocks;
- read-only update check, valid update, downgrade refusal, dirty source,
  unexpected remote/ref, non-fast-forward history, and invalid candidate;
- interruption before and after each activation write;
- lost response after a successful write and idempotent retry;
- rollback and stale/recoverable lifecycle locks;
- ordinary uninstall, user-modified target preservation, reinstall, and
  explicit purge preview;
- proof that uninstall/update never changes a project Memory Bank;
- proof that deleting all optional indices does not impair file-based resume;
- consistent revision and digest across every installed client.

No lifecycle test SHALL use real credentials, mutate a live client
configuration, alter a developer checkout, or depend on network access unless
it is an explicitly isolated channel contract test.
