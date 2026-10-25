# Clean Git checkout rejected by the runtime updater

The operator's installed v0.0.2 `am update` refused its source directory with
`Installed target readback failed`. Its previous update journal was complete;
this was not an interrupted activation. No project bank needed recovery.

## Root cause and diagnostic gap

The install receipt identifies an owned source target by hashing its entire
directory, including `.git/index`. Git can refresh that index's cached file
information during an ordinary clean `git status`. An isolated copy reproduced
this exact mechanism: status reported clean, every non-Git source file stayed
unchanged, the index changed, and the directory ownership hash changed. The
updater interpreted harmless Git bookkeeping as modified installed code.

The actual installation's 392 tracked files independently matched the bytes and
executable intent of recorded commit
`90dbf6559a026f180bd93eb460d08827fea240b2`. There were no extra non-Git files;
the recorded skill artifact digest also matched. The original historical index
bytes are unavailable, so these observations do not prove which operation first
changed the actual installation's opaque directory fingerprint. Copies of the
installation and complete lifecycle records were preserved before repair.

Doctor separately checked revision/channel/artifact but skipped the source
target's ownership hash, so it reported source health while update refused.
Update also printed generic interrupted-activation advice despite the completed
journal. Both paths must agree on the ownership basis and actual recovery state.

## Repair boundary

This corrects the existing ADR-0007 lifecycle guarantees and adds a real daily-use
regression to [#99](https://github.com/robertelee78/agent-memory/issues/99). It
does not replace that story's multi-session workload or claim v0.0.3 delivery.
The independently approved technical design is recorded in
[the design verdict](https://github.com/robertelee78/agent-memory/issues/99#issuecomment-6404642293).

Only a receipt-owned legacy `managed` source target may re-establish ownership
after its whole-directory hash changes. Its exact root mode, source/pack paths,
commit, branch, remote, version, compatibility and complete portable artifact
bytes/modes must still match. Every non-Git file must match the authenticated
recorded commit tree by raw blob bytes and owner-execute intent; every implied
directory must exist. Extra ignored files, empty directories, links, special
nodes, alternative object stores and actual source edits refuse and remain
preserved. Original commit and every traversed tree object are independently
hashed rather than trusted by object filename alone.

Git does not record all Unix permissions. This establishes Git-payload-equivalent
legacy ownership, not retrospective equality with the old opaque fingerprint.
The original receipt binds the root mode and exact portable artifact modes;
current trust-path rules qualify the rest. The complete freshly observed
directory fingerprint still binds all bytes and modes during staging, exchange,
backup and rollback. Existing receipts are not rewritten to bless a mismatch.

Local evidence uses sanitized read-only Git plumbing, disables replacements,
lazy fetching, fsmonitor, untracked cache and hooks, and never invokes `status`
or clean filters to prove a managed source clean. Remote acquisition retains its
existing transport authentication. Source blob reads use bounded ordinary-file
descriptors and reject replacement/growth races. This ownership fallback does
not apply to wrappers, copies, links, signed release sources or partial rollback.

The ordinary external installer/updater can stage and activate the correction;
installed code, old receipts and Git indexes must not be hand-edited. Backups
retain the complete actual prior directory. Read-only preview, full update,
repeat update, diagnostic consistency, actual interruption/rollback, real frozen
v0.0.2 transition and refusal controls are executable in
`tests/contracts/test_managed_source_ownership.py`. Source-bound validation
results belong on #99; existence of the tests alone is not a pass claim.

## Follow-up: deleted worktrees became explicit rollback inputs

After the ownership correction landed, the actual public one-line preview
succeeded and inspected 74 available banks. Apply then refused a deleted
registered worktree before changing targets. The compatibility observer correctly
reported missing registry/Git worktree locations as discovery gaps, but also put
them in `observed_repositories`. Activation reused that list as explicit
compatibility/rollback inputs, whose contract requires existing directories.
Thus preview and apply disagreed despite the same filesystem state.

The replay list now contains every repository directory actually observed,
including existing repositories without a bank. Missing discovered locations
remain reported gaps. An explicitly requested missing `--repo` still refuses;
an observed repository that disappears during a later check also refuses. No
registry entry or Git worktree metadata is pruned to repair an update. The
actual failed apply retained the previous completed journal and changed no
installed target. `test_update_discovery_gaps.py` exercises real stale Git
worktrees through check, update, repeat and recovery in link and copy modes,
alongside the explicit-missing and existing-bankless boundaries.
