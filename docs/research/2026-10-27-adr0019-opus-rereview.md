# Independent review of ADR-0019 (proposed)

**Verdict: accept once the required changes below are made.** The diagnosis is correct and the T1 recovery is conservative in the right way. It accepts only evidence that already exists and needs no Git writes. A few points are underspecified in ways that would either hurt users or let the implementation drift.

**Supporting the diagnosis:** 16777234 = `0x01000012` and 16777230 = `0x0100000E`, so the volume's minor device number changed from 18 to 14. On macOS, `dev_t` is assigned when a volume is mounted, so it can change across reboots when mount order changes. That fits "booted after registration". The ADR is right not to name a specific OS event and right not to treat `st_dev` as durable.

## Required before acceptance

1. **An exact match on the old key should still check the location.** "An exact current binding remains authoritative" trusts `st_dev:st_ino` alone. Device numbers get reused: an old one can later be assigned to a different volume. On APFS, inode numbers are small per-volume counters, so the same inode on another volume is plausible. An exact-key hit should therefore also require the same canonical common-Git location and project/bank UUID. If they don't match, treat it as unresolved, not as a match. This is the reverse of the problem being fixed and costs almost nothing to check.

2. **Say which key wins when there are several.** "Deterministic" needs a concrete rule, for example the key most recently written by a verified operation, with lexicographic order breaking ties. Without one, Claude, Codex and OpenCode could pick different keys from the same registry.

3. **Say how repair affects pending proposals.** The ADR says operation guards keep the effective key, and that repair must not rewrite prepared proposals. Neither says whether a guard checks the exact key or any key in the same equivalence group. If it checks the exact key, `doctor --fix` turns every pending proposal stale and the user has to prepare it again. That may be acceptable, but the ADR should say so and doctor should report it. Otherwise it should state that guards compare equivalence groups.

4. **Failure records collide.** "One record per location" combined with "a successful hook clears its own failure state" doesn't work. A successful prompt hook would erase a stop failure that is still pending, and one harness overwrites another's record. Key the records by (location, harness, adapter), or keep a small bounded map inside the single record. Also settle the scope: T3 names only the Codex adapters, while T4 says all three harnesses share the same identity rules. State whether Claude and OpenCode hooks write failure metadata in v0.0.7.

5. **Show that the missing boundary ledger came from the identity failure.** The context notes that no Codex boundary ledger existed and that the original failing event is gone. T4 should show that with a recovered binding, prompt → stop → acknowledgment actually creates the ledger. If that test isn't there, `boundary.missing` could be a second, independent bug that the identity fix only hides.

6. **Spell out what each amended ADR loses.** Name the exact statements in ADR-0004, ADR-0011 and ADR-0018 that are replaced, especially any that define identity as `st_dev:st_ino`.

## Recommended

- **Bound the history.** Repair keeps history, so every device change adds a key. Set a cap or a pruning rule, for example keep the N most recent equivalent keys per project.
- **Optionally use birth time as extra evidence.** `st_birthtime` on macOS, or `statx` btime on Linux, for the common Git directory would make "replaced clone at the same path" much harder to confuse with continuity, without any writes. On Linux ext4, inodes are reused after `rm -rf` and re-cloning, so inode plus path alone is weaker there.
- **Define the threat model for re-clones.** Deleting a repo and re-cloning it at the same path brings back the same bank UUID and remote. If the inode is ever reused, it passes every T1 check. Decide whether that counts as continuity or as an independent clone, and write the decision down.
- **Keep hooks quiet when they recover.** Each turn should continue silently with recovered bindings, apart from at most a short one-line hint, and leave the full explanation to `am doctor`. That matches the "routine success should be quiet" rule.
- **Handle cases that are expected to fail.** A repo that was moved and then had its device number change, or whose remote changed from https to ssh, will correctly stay unresolved. Doctor should give a specific relink message for both. Also normalize remotes so that changing only the transport doesn't cause a refusal.
- **Test with a real device change.** On macOS, attach a disk image with `hdiutil attach`, bind a repo on it, detach, attach a different image first, then reattach. The original image usually comes back with a different `st_dev`. Use stat injection on Linux, since loop mounts need root.
- **Cover submodules.** A `.git` file that points into `superproject/.git/modules/…` should be listed next to linked worktrees under the common-Git-directory rule.

## Rejected alternatives

The reasoning holds up. On macOS, `statfs` `f_fsid` comes from the same unstable device number, so a stable volume ID would need `getattrlist` (`ATTR_VOL_UUID`) or a call to `diskutil`. That is extra platform-specific code, as the ADR says.

## Process gaps in this session

- **Tracking and memory checks weren't run.** This session only had the IOA Brain tools. I had no shell or file access, so I couldn't run `awa status --json` or load the `github-work-accountability` and `agent-memory` skills. Issue/Project reconciliation and memory capture for this review are still pending and need to be run from a session with shell access.
- **This is a review of the ADR's design only.** I didn't read the source of `project_identity.discover`, `scope_binding.require` or the hook adapters. Check findings 1 and 3 against the code before you accept the ADR.
