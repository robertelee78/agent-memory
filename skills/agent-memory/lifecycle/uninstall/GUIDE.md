---
kind: operation
domain: maintain
---

# Uninstall owned runtime targets

## Use when

A session MUST use this operation when the operator requests runtime removal.
Ordinary uninstall does not delete project memory, projections or backups.
Data deletion has a separate [purge operation](../purge/GUIDE.md) with explicit
inventory and confirmation; uninstall authorization MUST NOT imply purge consent.

## Required context

The session MUST first load the [maintain domain](../../maintain/GUIDE.md).
It MAY consult the [shared model](../../model/GUIDE.md) for unresolved memory
meaning. Runtime removal does not require principal setup or a project bank.

## Inputs and preconditions

Before removing managed code, the session MUST resolve `SKILL_DIR` to a trusted
complete package outside the targets being removed, or retain a way to obtain
that package. Its source helper survives removal of the installed wrapper and
provides recovery, repeat and reinstall without relying on deleted code.
Python 3.11 or later and the existing external ownership receipt are required.

## Observe

The session MUST inspect the external installation receipt, owned target
identities, activation block hashes and retained backups. Missing source is
different from missing ownership: the trusted helper can remove surviving
owned targets when managed source is missing, but MUST NOT guess targets when
the ownership receipt is missing or invalid. Explicit developer sources remain
outside removal ownership.

## Plan

The session MUST preview removal before applying it:

```sh
python3 -I "$SKILL_DIR/scripts/uninstall_runtime.py" uninstall --check > /private/path/uninstall-plan.json
```

The session MUST review each target's before/after identity and the `preserved`
list. Check mode writes no targets or records. Changed copies, links, wrappers
or activation blocks remain preserved; a preserved client target can keep
managed source in place conservatively because it may still reference it.

## Apply

After an authorized acceptable preview, the session MUST run the same helper
without `--check`, or the installed `agent-memory uninstall` command:

```sh
python3 -I "$SKILL_DIR/scripts/uninstall_runtime.py" uninstall > /private/path/uninstall-receipt.json
```

The lifecycle lock, external journal and native per-target moves retain rollback
material. Managed source is removed only after the selected client targets no
longer need it. Separate target filesystems MUST NOT be described as one atomic
global uninstall.

## Read back

The session MUST require exit 0 and `verified: true`, then inspect `preserved`,
`receipt`, `reinstall`, `project_memory_writes_performed` and `purge_performed`.
`uninstalled: true` decommissions active installer ownership; it MUST NOT be
reported as complete removal when `preserved` still lists user-modified targets.
Those paths and their reasons require prompt, concrete reporting.

## Verify

The session MUST verify the recorded removed/restored target results and retained
data. An unchanged activation file can restore proven original bytes from its
backup; later outside text remains preserved when removing the owned block.
Modified blocks remain untouched. Uninstall retains a minimal inactive receipt
and backups; inactive ownership permits a fresh installation instead of reading
Git metadata from the removed source.

## Outcomes

The session MUST report removal, preserved conflicts and the returned reinstall
recipe separately. The recipe's `${SKILL_DIR}` placeholder MUST be resolved to
an available trusted complete package before invoking its `argv`. The
[installation operation](../install/GUIDE.md) then previews and reinstalls the
recorded channel or developer source and targets against the existing bank;
it MUST NOT initialize replacement memory. Repeating the trusted uninstall
helper converges without re-deleting preserved data.

## Failure handling

On refusal or interrupted removal, the session MUST preserve receipt, journal,
staging paths, backups and user edits. It MUST NOT force-remove a changed target
or delete data to make a retry pass. A live lifecycle lock remains busy; a
timestamp or PID alone does not authorize stealing it.

## Diagnostic fallback

The session MUST recover interrupted runtime removal using the trusted complete
package, even when the installed command and managed source are gone:

```sh
python3 -I "$SKILL_DIR/scripts/installer.py" recover
```

The session MUST read the verified recovery result before retrying uninstall.
Recovery restores an incomplete removal or verifies its committed receipt;
external edits discovered during recovery remain preserved for reconciliation.
