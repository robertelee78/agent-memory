---
kind: operation
domain: maintain
---

# Explicitly purge one local project bank

## Use when

A session MUST use this operation only for an explicit request to delete a
resolved local Memory Bank. Ordinary uninstall, cleanup, pruning and forgetting
a lesson MUST NOT be treated as authorization to purge an entire bank.

## Required context

The session MUST first load the [maintain domain](../../maintain/GUIDE.md).
It MAY open the [shared model](../../model/GUIDE.md) for unresolved ownership.
The [uninstall operation](../uninstall/GUIDE.md) preserves banks and is independent
of purge. A trusted complete package can purge after runtime uninstall.

## Inputs and preconditions

The session MUST identify an explicit absolute repository root and the exact
bank selected by the existing one-bank locator. The helper refuses a broad
directory, repository/home/filesystem root, symlink traversal, nested Git
metadata, incompatible bank selection or pending memory transaction. Initial
support requires a recognizable ordinary v1 router and bounded ordinary data.

## Observe

The session MUST preserve any current bank writer or pending transaction and
finish its own supported recovery before planning deletion. The purge preview
classifies durable and generated bank files and identifies retained runtime,
installation state, Git history and external backups. Deleting this local bank
does not erase remote copies, other branches, other worktrees or Git history;
the session MUST explain that scope when presenting the deletion request.

Purge MUST also inspect the private recovery index and state binding before
preview and under the bank lock. Unknown, interrupted or unresolved recovery
dependencies refuse deletion even if a safeguard was removed manually. Purge
MUST NOT delete external originals or resolve pending knowledge. Terminal
recovery evidence requires the separate reviewed operation-scoped
`am bank recover-layout retention inspect|apply` workflow; age and layout
success alone are insufficient.


## Plan

The session MUST obtain a read-only exact preview, with `SKILL_DIR` resolved to
a trusted complete package:

```sh
python3 -I "$SKILL_DIR/scripts/uninstall_runtime.py" purge-data --repo /absolute/repository --path /absolute/repository/memory-bank --check > /private/path/purge-preview.json
```

The example bank path MUST be replaced by the repository's actual resolved bank,
including a configured custom root. The preview binds all listed names, types,
modes, contents, counts and physical identities to `preview.confirmation`.

## Apply

Before invoking deletion, the session MUST present the exact resolved paths,
counts, durable/generated classification and retained data to the operator and
obtain explicit confirmation for that preview. A generated digest or elapsed
time MUST NOT be treated as operator approval. With the confirmed digest, it
MUST use the same repository/path selection and replace
`CONFIRMED_PREVIEW_SHA256` with that operator-confirmed digest:

```sh
python3 -I "$SKILL_DIR/scripts/uninstall_runtime.py" purge-data --repo /absolute/repository --path /absolute/repository/memory-bank --confirm CONFIRMED_PREVIEW_SHA256 > /private/path/purge-receipt.json
```

The installed `agent-memory purge-data` command uses the same options. Changed
contents or physical identities require a fresh preview and confirmation;
the session MUST NOT automatically substitute a newly generated digest.

## Read back

The session MUST require exit 0 and `verified: true`, then inspect the receipt,
its exact confirmed inventory, deletion readback and `replacement_bank_present`.
An inactive purge receipt is historical evidence for its confirmed physical
bank, not authority to delete later memory at the same path.

## Verify

Purge takes the lifecycle and bank locks, rechecks the confirmed inventory,
journals it outside the bank, and detaches that exact bank into a same-parent
quarantine before removing approved remaining entries. The session MUST NOT
describe this as global atomic deletion or secure erasure. Only the selected
local bank is deleted; runtime, unrelated projects, staged code/index, Git
history and external backups remain outside deletion scope.

## Outcomes

The session MUST report the confirmed bank's deletion separately from retained
data. If a new bank appeared at the canonical path, retry preserves it and
reports `replacement_bank_present: true`; the session MUST NOT claim that the
current canonical bank is absent. A completed repeat does not delete a newly
created bank or silently extend prior consent to its files.

## Failure handling

On interruption, refusal or unexpected quarantine contents, the session MUST
preserve the quarantine, external purge journal and any new canonical bank.
It MUST NOT recursively delete an unverified directory, remove a foreign lock,
or overwrite new data to finish the operation. New or changed quarantine entries
require reconciliation rather than deletion under the old confirmation.

## Diagnostic fallback

After the original writer has ended, the session MUST inspect the retained
external record under the installation state's `purges/` directory and retry
the same already confirmed command to finish its recorded quarantine. The
helper checks the original root/entry identities and stale purge lock ownership.
Runtime `installer.py recover` is for runtime activation and MUST NOT be used
as a substitute for this separately confirmed data operation. An unreadable,
linked or mismatched purge journal requires bounded manual reconciliation;
the session MUST NOT guess another deletion target.

### Recovery evidence is separate from bank purge

The session MUST NOT use bank purge, runtime update or uninstall to erase external
layout-recovery evidence. Unknown/pending recovery operations, an unresolved
frontier or a live restoration/publication dependency MUST refuse bank purge.
Runtime-owned paths do not include the private recovery workspace.

Only `am bank recover-layout retention inspect --repo R --operation ID --output O`
may preview operation-scoped evidence cleanup. The preview enumerates exact
owned files, bytes, physical/hash pins, child/nested/publication dependencies,
operator retention requirements, retained receipts and rollback capability lost.
Age, layout success and terminal units alone MUST NOT authorize deletion.
Unknown backups, unindexed operations, missing/corrupt originals and unfinished
publication/resolution/restore work remain pending.

The session MUST obtain a separate independent dependency review and explicit
operator acknowledgment of rollback loss. The private
`agent-memory/layout-retention-review-v1` binds the exact preview with its new,
stable cleanup UUID. The session MUST pass the resulting unchanged proposal to
`am bank recover-layout retention apply --repo R --proposal P`; a generated
digest, copied `approved` heading or elapsed time is not authorization.

Apply writes a durable cleanup intent and truthful tombstone before retiring the
terminal index and deleting the first individually listed file. It checks exact
owner/type/link/bytes/physical and parent bindings before each unlink, fsyncs and
records immutable progress. It never recursively deletes directories, follows
redirects, deletes unresolved units or removes a live sentinel. An interrupted
cleanup is resumed only with the exact same proposal and attainable remaining
file state; unknown/replaced files stay untouched.

The original completed receipt, resolution/nested historical receipts and
publication requests/landing receipts remain private and available for exact
historical retries. The safe tombstone says original bytes and future rollback
are unavailable by reviewed cleanup; it MUST NOT call removed backups verified,
rewrite an old layout receipt as later admission, or imply secure erasure.

Known typed partial-staging metadata MUST remain retained with
`observation_complete:false`, even when a later complete manifest exists.
It MUST NOT become authority for semantic completion or be silently deleted.
Malformed, foreign or unknown staging metadata remains a cleanup refusal.

A completed child restore MUST be checked through its typed child proposal and
real parent/child receipts and original parent-head pins. It has no invented
child pending ledger or canonical admission. Dependency discovery inventories the
union of physical operations, private bindings and the real operation index;
missing/unknown entries and unfinished children refuse cleanup. An active parent
retains its dependencies. After verified child completion, separately reviewed
cleanup may surrender its rollback bytes while retaining both historical
proposals/receipts. Retention success itself MUST NOT claim semantic recovery
completion or make a restored invalid bank healthy.

Quiescence heads, their referenced attestations and original/renewal reviews
MUST remain retained together; cleanup MUST NOT leave a retained head pointing
at evidence it removed.
