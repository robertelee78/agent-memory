# Checkout identity and Codex boundary RCA

## Evidence and causal chain

Observed on the operator Mac, 2026-10-27, installed v0.0.6:

| Repository | Registered device:inode | Current device:inode | Result |
|---|---|---|---|
| agent-memory | 16777234:419497488 | 16777230:419497488 | binding_required |
| ioa-brain | 16777234:115288067 | 16777230:115288067 | binding_required |
| cve-to-exploit | 16777234:288274036 | 16777230:288274036 | binding_required |
| vox | both old and current device, inode 199626517 | 16777230:199626517 | bound after separate explicit link |

All corresponding registry locations remained the same. Existing bank UUIDs and
registered canonical remotes matched. Registry records date to October 25; the
Mac booted October 27 10:04:25 local time. A reboot/remount causing the device
change is plausible, not established OS-level causation.

`project_identity.discover` builds a persistent key using st_dev and st_ino.
`evidence`, `status`, `plan` and quick diagnostics use exact-key lookup. Thus
normal filesystem identity churn becomes an apparent independent clone. The
project binding was not deleted: its key stopped matching.

Codex prompt calls scope_binding.require before creating its private pending
record. Stop calls the same guard before reading that record. With unresolved
identity, both abort. No codex-boundaries directory was found for this checkout.
A constructed event with UUID session/turn and valid cwd/final response reproduced
the warning. This is a controlled reproduction, not the original event replay.
The warning supplied by the operator proves the installed Stop hook ran, not
that it reached capture, assessed learning, or published anything.

## Why it escaped

The project tests exercise separate clones, shared worktree Git directories and
same-filesystem moves. The Codex test uses a newly bound fixture and synthesized
UUID event values. Those cases do not exercise device-number churn after a
previous registration. Diagnostics independently implement exact-key lookup,
then classify the bank as never adopted instead of recognizing historical local
evidence. doctor --fix repairs runtime ownership and receipted bank transactions,
but deliberately omits identity recovery. Hook catch-alls discard the failed stage
and return one generic message. Successful exit status therefore masks multiple
pending failure classes.

## Additional limits and risks

The pending ledger is keyed by bank path rather than device number, so existing
records need not be lost on device churn. Here prompt binding failed before a
record could be created. Repairing identity cannot reconstruct an absent source
record: a new actual prompt is needed, never a fabricated acknowledgment.
The adapters require UUID-shaped IDs although the public hook contract describes
strings. That is a compatibility risk to verify against actual events, not a
proven cause of this incident. The original event was not retained, so its first
failed check cannot now be proven. The user main checkout lags origin/main;
installed code is independently v0.0.6, so that lag is not the identity cause.

## Corrective scope

ADR-0019 defines conservative recognition of the same historical checkout,
verified persistence through doctor --fix, actionable private hook diagnostics,
and focused regression/real-harness checks. Device number alone must not define
durable project continuity; remote equality alone must not adopt a clone.
No bank corruption, deletion, unauthorized publication or memory transfer was
observed. No claim is made that pending capture completed.
