# Owned runtime removal and explicit local data purge

`am uninstall --check` previews the external receipt's owned targets.
`am uninstall` removes only unchanged ownership and decommissions the
active installation (`src/uninstallation.rs`). A separately downloaded and
verified `am` binary runs the same `uninstall [--check]` after the installed
command or owned runtime is gone. No project bank or principal setup is needed
for runtime removal. A Python-era receipt refuses with re-bootstrap guidance.

Uninstall preserves project memory, projections, unrelated settings and skills,
outside-block instruction edits, explicit developer sources and existing backups.
Changed copies, links, wrappers or activation blocks remain untouched and appear
in `preserved`; managed source stays conservatively when preserved clients may
still reference it. `uninstalled: true` means active ownership is decommissioned,
not that user-modified paths were deleted. The operator must see those paths and
the returned reinstall recipe. A missing ownership receipt never authorizes
guessing or deletion. A valid receipt with a missing runtime can still remove
the remaining verified targets through a separately verified `am` binary.

Removal uses the existing stable lifecycle lock and external transaction journal.
Each removed node moves atomically to a same-parent rollback stage. Restored
activation files use per-target exchange. Exact original bytes are restored only
when validated backups and current surroundings prove later edits cannot be
lost. The engine reads back removed/restored targets before publishing an
inactive uninstall receipt. Prior installation receipts and backups remain
available. `am recover` rolls back interrupted removal or
verifies its commit point; retry converges, including a lost success response.
Separate targets do not form one global atomic switch.

The inactive receipt carries source and target information for reinstall: an
`am install` argument list with the recorded mode, presets, bin directory,
custom targets and guidance choice. A signed release is reinstalled by running
the trusted bootstrap with those options; a development source needs a trusted
`am` built from it (`target/release/am`). Preview the returned installation
configuration before applying it. A fresh installation uses the preserved bank
rather than initializing replacement memory. The bootstrap downloads and
verifies a new binary, so recovery and reinstall do not depend on deleted code.

Data deletion is separate:

```text
agent-memory purge-data --repo /absolute/repo --path /absolute/repo/memory-bank --check
agent-memory purge-data --repo /absolute/repo --path /absolute/repo/memory-bank --confirm CONFIRMED_PREVIEW_SHA256
```

A separately verified `am` binary supports the same `purge-data` subcommand. Replace the example
bank path with the existing one-bank locator's actual result, including custom
roots. Preview lists the exact repository/path, project ID, all relative entries,
file/directory/byte counts, durable/generated classification and a confirmation
digest. The digest binds content, modes, types and physical identities. The skill
requires explicit operator approval of those exact paths/counts before deletion;
uninstall consent or an automatically computed digest is insufficient.

Purge refuses broad/home/filesystem-root selections, symlink traversal or entries,
nested Git metadata, another writer's lock, pending memory transactions and an
unrecognized router. Inventory is bounded to 40,000 entries, 64 MiB per ordinary
file, 512 MiB total and a bounded external journal; it never silently truncates
consent. Unsupported or changed input requires inspection rather than guessing.

Confirmed purge takes both the lifecycle lock and actual bank lock, rechecks the
selection/inventory, journals outside the bank, and atomically detaches that
physical bank into a same-parent quarantine. It removes only approved remaining
entries, checks identities before removal and records deletion readback. Retrying
the same confirmed operation resumes its recorded quarantine, including an
interrupted partial deletion or lost response. New or changed quarantine data is
preserved. A newly recreated canonical bank remains untouched and is reported by
`replacement_bank_present`; old consent does not authorize deleting it.

Approved hardlink aliases retain the same consent boundary. Write-ahead unlink
intent and recorded self-induced timestamp changes let recovery account for an
alias removed immediately before interruption, without accepting changed file
contents, modes, modification times or inode identities under the original approval.

The result concerns only the confirmed local bank. Runtime code, installation
state, unrelated projects, staged code/index, external backups, remote copies and
Git history are retained. This is not secure erasure or a globally atomic deletion.
`am recover` handles runtime transactions; purge recovery uses the same
already-confirmed purge command and its separate `purges/` journal. Linked,
unreadable or mismatched journals require bounded reconciliation.

The [uninstall guide](../../skills/agent-memory/lifecycle/uninstall/GUIDE.md) and
[purge guide](../../skills/agent-memory/lifecycle/purge/GUIDE.md) provide agent
workflows. Uninstall and purge tests in `tests/t2_lifecycle.rs` exercise real
public operations with isolated homes/client roots and local Git. Full all-operation
write-boundary testing and the portable lifecycle release matrix remain their
separate #60 and #52 deliverables.
