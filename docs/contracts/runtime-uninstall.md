# Owned runtime removal and explicit local data purge

`agent-memory uninstall --check` previews the external receipt's owned targets.
`agent-memory uninstall` removes only unchanged ownership and decommissions the
active installation. The complete portable package also contains
`scripts/uninstall_runtime.py uninstall [--check]`, which remains usable after
the installed command or managed source is gone. No project bank or principal
setup is needed for runtime removal.

Uninstall preserves project memory, projections, unrelated settings and skills,
outside-block instruction edits, explicit developer sources and existing backups.
Changed copies, links, wrappers or activation blocks remain untouched and appear
in `preserved`; managed source stays conservatively when preserved clients may
still reference it. `uninstalled: true` means active ownership is decommissioned,
not that user-modified paths were deleted. The operator must see those paths and
the returned reinstall recipe. A missing ownership receipt never authorizes
guessing or deletion. A valid receipt with missing source can still remove the
remaining verified targets through the trusted helper.

Removal uses the existing stable lifecycle lock and external transaction journal.
Each removed node moves atomically to a same-parent rollback stage. Restored
activation files use per-target exchange. Exact original bytes are restored only
when validated backups and current surroundings prove later edits cannot be
lost. The engine reads back removed/restored targets before publishing an
inactive uninstall receipt. Prior installation receipts and backups remain
available. The trusted installer's `recover` rolls back interrupted removal or
verifies its commit point; retry converges, including a lost success response.
Separate targets do not form one global atomic switch.

The inactive receipt carries channel/developer-source and target information for
reinstall. Resolve its `${SKILL_DIR}` placeholder to an available trusted complete
package, then preview the returned installation configuration before applying it.
A fresh installation uses the preserved bank rather than initializing replacement
memory. Keep a trusted package outside removed targets or a way to obtain it
before uninstalling, so recovery and reinstall do not depend on deleted code.

Data deletion is separate:

```text
agent-memory purge-data --repo /absolute/repo --path /absolute/repo/memory-bank --check
agent-memory purge-data --repo /absolute/repo --path /absolute/repo/memory-bank --confirm CONFIRMED_PREVIEW_SHA256
```

The portable helper supports the same `purge-data` subcommand. Replace the example
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

The result concerns only the confirmed local bank. Runtime code, installation
state, unrelated projects, staged code/index, external backups, remote copies and
Git history are retained. This is not secure erasure or a globally atomic deletion.
`installer.py recover` handles runtime transactions; purge recovery uses the same
already-confirmed purge command and its separate `purges/` journal. Linked,
unreadable or mismatched journals require bounded reconciliation.

The [uninstall guide](../../skills/agent-memory/lifecycle/uninstall/GUIDE.md) and
[purge guide](../../skills/agent-memory/lifecycle/purge/GUIDE.md) provide agent
workflows. [Uninstall fixtures](../../tests/contracts/test_runtime_uninstall.py)
and [purge fixtures](../../tests/contracts/test_runtime_purge.py) exercise real
public commands with isolated homes/client roots and local Git. Full all-operation
write-boundary testing and the portable lifecycle release matrix remain their
separate #60 and #52 deliverables.
