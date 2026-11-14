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
remains. An interrupted installation or update MUST be resolved through the
[installation operation](../install/GUIDE.md). Its explicit re-plan route permits
a fresh preview when an exact committed receipt needs allowed surrounding edits
reconciled; it does not turn strict recovery's refusal into verified success.

## Inputs and preconditions

The session MUST identify the external installation receipt and the recorded
source kind and channel. The bootstrap-installed `am` command (checked with
`am --version`) and Git are required; signed acquisition additionally requires
curl and OpenSSH SSH-signature support.
An explicit development source MUST NOT be updated by this operation. The signed default requires the pinned publisher signature before candidate
execution. Explicit Git channels and old-process first activations remain
unsigned; a digest or fast-forward alone does not authenticate the publisher. Optional agent client executables are not prerequisites.

## Observe

The session MUST inspect the current receipt and installation ownership. The
updater checks source cleanliness, exact revision/artifact identity, recorded
origin/ref, client links/copies, wrapper and owned activation blocks. User text
outside those blocks remains outside installation ownership. The candidate
[activation template](../../templates/activation-markdown.example.txt) is package
data; the session MUST inspect it only when planning or verifying a changed
activation block, rather than copy the memory protocol into client settings.

## Plan

With the trusted installed `am` command, the session MUST preview
an update before activation:

```sh
am update --check > /private/path/update-check.json
```

The session MAY add repeated `--repo /absolute/repository` arguments for relevant
repositories beyond the current directory and locally registered checkouts.
The updater uses the existing one-bank locator for each repository. A discovered
bank outside the candidate's readable and bundled migrator source declarations
MUST prevent activation. Initial v1 declares no migrators. Unavailable registered
locations remain explicit discovery gaps, and later ordinary bank use still
requires its own schema guard.

Compatibility MUST also preserve the union of every discovered bank's active
`private_state_capabilities`, including `layout-frontier-v1` and any independent
native source requirement. Missing legacy capability metadata means an empty
set, never inferred support. The inventory includes local projects, saved
bindings, recovery indexes/holds and native source registries; inaccessible,
offline or corrupt private state leaves activation pending. Clearing one
feature's requirement MUST NOT clear another's.

An enabled stored-source route requires both `omp-harness-v1` and
`omp-stored-source-v2`. Upgrading an older native binding MUST include a renewed
`--omp-peers-upgraded`; a saved legacy approval is not sufficient. A committed
update with `stored_source_activation: pending` MUST retain its runtime and
private evidence. The session MUST restore unavailable banks, quiesce writers
and run `am recover`; it MUST NOT roll back the committed receipt to hide the
pending activation or count the update as verified.

A pending layout frontier requires the exact bound state and independently
reviewed operator quiescence for the measured target runtime and all same-bank
invocation routes. Update, recovery and exact rollback recheck these pins before
activation and readback. An incapable old rollback target MUST NOT be activated
merely to finish a lifecycle journal. External recovery originals and ledgers
are not installer-owned and remain intact.
Use repeatable `--layout-quiescence /private/path/renewal.json` for each affected
bank when the measured candidate or invocation routes differ from the retained
review. Each protected file contains the exact `attestation` and independent
`review` envelope described by layout recovery. Check mode validates without
publishing renewals; apply durably retains them under the same source barriers
before the final capability check. A capability declaration alone is not renewal.



## Apply

After an acceptable authorized check, the session MUST invoke the same trusted
helper without `--check`:

```sh
am update > /private/path/update-receipt.json
```

The installed `agent-memory update` command dispatches this operation. If an
older installed command cannot activate the native runtime, the session MUST
rerun the verified bootstrap. It MUST NOT edit or pull into the active managed
checkout to bootstrap that command.

## Read back

The session MUST read the actual result and distinguish `current`, `available`,
`update_available`, `loaded_content_changes`, compatibility, receipt and restart
list. Check mode is read-only for targets, receipts, registries, banks and active
Git metadata; temporary signed staging or a custom-channel clone is removed
after inspection. This does not claim zero temporary filesystem writes.

## Verify

The session MUST require exit 0 and `verified: true` after activation, or
`check_complete: true` for a successful preview. The updater stages a complete
candidate without changing active source. It proves signed-release version/identity
and native target/command-surface compatibility, validates the package, then uses
the owned per-target
transaction. It verifies client-visible packages and the actual installed
command before publishing the new external receipt. Native exchange and the
journal MUST NOT be described as a single global atomic switch.

## Outcomes

A validated owned official legacy main installation automatically adopts signed
releases. A newer version is required unless portable paths/bytes and Git executable
intent match exactly; old receipt/modes are verified separately and never altered.
An already-signed version cannot be replaced with different identity. The old
process first activation remains unsigned; custom channels never silently change.


A successful update preserves project Memory Banks and reports their inspection
separately from code activation. The session MUST NOT infer that runtime update
migrated any bank or reloaded a running client. Running clients SHOULD restart
only when listed because their loaded content changed. An unchanged repeat or
an outside-package code revision produces no unnecessary restart advice.

### Opaque cache drift

Unexpected runtime additions remain ownership drift. A session MUST preserve
the affected runtime and external receipt for independently trusted inspection;
it MUST NOT remove unexpected files or weaken exact runtime verification to
force an update. Transition from an older runtime requires the operator-trusted
verified bootstrap. After committed adoption, an older runtime MUST NOT be
reactivated or used as a verification step. Project memory and pending recovery
evidence remain outside runtime replacement ownership.

## Failure handling

The session MUST preserve a dirty source, changed origin/ref, downgrade,
non-fast-forward candidate, invalid package, ownership change or incompatible
bank and resolve the reported condition before retrying. Failure before
activation leaves the prior runtime active. Partial activation rolls back or
retains an explicit external journal; user edits discovered during recovery
MUST remain preserved. A lost response does not undo a committed receipt.
Verification requires the exact committed receipt. Clean managed Git metadata
MAY independently requalify against the unchanged payload, modes and channel;
other changed targets can require the installation operation's explicit
ownership re-plan. Repeating update converges
without duplicate activation blocks; the old pending journal remains retained
as archive evidence.

## Diagnostic fallback

When activation is interrupted, the session MUST invoke the trusted installer
helper's `recover` operation, inspect its readback, then retry this operation.
It MUST correct an unsafe code/state/bank layout before invoking recovery under
that layout. The shared lock keeps one OS-lock inode and atomically publishes
its process/start metadata beside it, so killed metadata writers do not tear
the lock's ownership marker. The session MUST treat an actual held lock as busy,
not remove it because a PID or old timestamp looks stale.
