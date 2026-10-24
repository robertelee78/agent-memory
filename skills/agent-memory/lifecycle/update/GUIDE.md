---
kind: operation
domain: maintain
---

# Update the managed runtime

## Use when

A session MUST use this operation to check or activate the installation's
recorded managed channel. Updating runtime code does not capture, publish,
merge, prune or migrate project memory.

## Required context

A session MUST first load and follow the [maintain domain](../../maintain/GUIDE.md).
It MAY open the [shared model](../../model/GUIDE.md) when a memory meaning question
remains. An interrupted installation or update MUST be recovered through the
[installation operation](../install/GUIDE.md) before another check can proceed.

## Inputs and preconditions

The session MUST identify the external installation receipt and the recorded
managed source, origin and branch. Python 3.11 or later and Git are required.
An explicit development source MUST NOT be updated by this operation. The
initial Git channel is unsigned; a valid digest or fast-forward does not prove
publisher authenticity. Optional agent client executables are not prerequisites.

## Observe

The session MUST inspect the current receipt and installation ownership. The
updater checks source cleanliness, exact revision/artifact identity, recorded
origin/ref, client links/copies, wrapper and owned activation blocks. User text
outside those blocks remains outside installation ownership. The candidate
[activation template](../../templates/activation-markdown.example.txt) is package
data; the session MUST inspect it only when planning or verifying a changed
activation block, rather than copy the memory protocol into client settings.

## Plan

With `SKILL_DIR` resolved to a trusted complete package, the session MUST preview
an update before activation:

```sh
python3 -I "$SKILL_DIR/scripts/update_runtime.py" update --check > /private/path/update-check.json
```

The session MAY add repeated `--repo /absolute/repository` arguments for relevant
repositories beyond the current directory and locally registered checkouts.
The updater uses the existing one-bank locator for each repository. A discovered
bank outside the candidate's readable and bundled migrator source declarations
MUST prevent activation. Initial v1 declares no migrators. Unavailable registered
locations remain explicit discovery gaps, and later ordinary bank use still
requires its own schema guard.

## Apply

After an acceptable authorized check, the session MUST invoke the same trusted
helper without `--check`:

```sh
python3 -I "$SKILL_DIR/scripts/update_runtime.py" update > /private/path/update-receipt.json
```

The installed `agent-memory update` command dispatches this operation. A trusted
new source helper remains available when an older installed command cannot yet
dispatch update. The session MUST NOT edit or pull into the active managed
checkout to bootstrap that command.

## Read back

The session MUST read the actual result and distinguish `current`, `available`,
`update_available`, `loaded_content_changes`, compatibility, receipt and restart
list. Check mode is read-only for targets, receipts, registries, banks and active
Git metadata; a temporary candidate clone is removed after inspection.

## Verify

The session MUST require exit 0 and `verified: true` after activation, or
`check_complete: true` for a successful preview. The updater stages a complete
candidate without fetching into the active checkout, proves fast-forward/version
and Python compatibility, validates the package, then uses the owned per-target
transaction. It verifies client-visible packages and the actual installed
command before publishing the new external receipt. Native exchange and the
journal MUST NOT be described as a single global atomic switch.

## Outcomes

A successful update preserves project Memory Banks and reports their inspection
separately from code activation. The session MUST NOT infer that runtime update
migrated any bank or reloaded a running client. Running clients SHOULD restart
only when listed because their loaded content changed. An unchanged repeat or
an outside-package code revision produces no unnecessary restart advice.

## Failure handling

The session MUST preserve a dirty source, changed origin/ref, downgrade,
non-fast-forward candidate, invalid package, ownership change or incompatible
bank and resolve the reported condition before retrying. Failure before
activation leaves the prior runtime active. Partial activation rolls back or
retains an explicit external journal; user edits discovered during recovery
MUST remain preserved. A lost receipt response is verified as committed and
repeating update converges without duplicate activation blocks.

## Diagnostic fallback

When activation is interrupted, the session MUST invoke the trusted installer
helper's `recover` operation, inspect its readback, then retry this operation.
It MUST correct an unsafe code/state/bank layout before invoking recovery under
that layout. The shared lock keeps one OS-lock inode and atomically publishes
its process/start metadata beside it, so killed metadata writers do not tear
the lock's ownership marker. The session MUST treat an actual held lock as busy,
not remove it because a PID or old timestamp looks stale.
