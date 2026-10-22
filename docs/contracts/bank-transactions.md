# Interrupted multi-file bank writes

The common transaction helper supplies per-file atomic replacement, an explicit
interruption marker, and observed finish/rollback for an existing valid bank.
Consolidation, correction and pruning callers MUST use it when changing several
canonical files. Their separate admission, deletion, identity and evidence rules
still apply. This mechanical helper does not authorize a remembered instruction,
prove a developer's identity, publish Git changes, or provide cross-file atomicity.
First bank creation is owned by the initialization operation; this helper requires
an existing valid bank and has no installed CLI or developer-registry prerequisite.

## Preview and apply

```sh
python3 -I scripts/bank_transactions.py preview --repo /path/to/repository \
  --operation compact --files /tmp/desired-bank-files.json
python3 -I scripts/bank_transactions.py apply --proposal /tmp/transaction.json
```

The files input is a JSON object mapping every desired canonical relative
Markdown path, including `MEMORY.md`, to its complete UTF-8 text. Missing former
paths mean deletion. Preview validates the projected bank with the same router
contract as ordinary readers, emits an operation UUID and full observed hash map,
and writes nothing. The complete map makes source conditions part of the read
set. It is temporary operation input, not another project memory store.

Apply MUST acquire the common exclusive bank lock before inventory. It refuses a
changed project ID or base, unknown receipt, interruption marker, unsafe path or
symlink. Only an explicitly named `bind` operation can change project identity;
the governed binding caller supplies that decision. Receipts, journals and
backups use the configured state root outside the current repository. State
paths MUST NOT traverse symlinks. Journal files are private (`0600`). The journal
holds the observed base and intended text so a torn update can be recovered.

While locked, the helper creates `.agent-memory-transaction.md` using exclusive,
no-replace semantics. This JSON-in-Markdown runtime marker contains a random
transaction ID, relative affected paths, base hashes, operation and `applying`
phase. It MUST NOT contain memory bodies, deleted prose, source excerpts,
credentials or other secret content. The phase describes an outstanding update;
it is not a claim that every target has changed.

Each target uses a same-directory staged, flushed atomic replacement (or an
observed unlink for deletion). The writer checks the full read set and marker
immediately before every change. `MEMORY.md` changes last. It verifies every
resulting canonical path and the complete projected bank, records the readback
receipt while locked, then removes the marker and flushes the bank directory.
Only then does it discard its exact known journal/backups. Unrelated state files
are preserved. Completed receipts contain operation/hash/path metadata, not
memory bodies; the governed forget operation must also purge its bounded
sensitive artifacts under its separate erasure contract.

## Recovery

Readers and normal mutations MUST stop when the transaction marker is present.
Deleting an index or state directory does not repair an interrupted bank.

1. Inspect a remaining bank lock with `bank_writes.py recover`. Apply its reviewed
   proposal only when the same-host owner is provably gone. Live, foreign or
   unknown locks require operator inspection; age is not proof.
2. Choose a direction explicitly and inspect a read-only proposal:

   ```sh
   python3 -I scripts/bank_transactions.py recover --repo /path/to/repository --mode finish
   # or --mode rollback
   ```

3. Apply that exact proposal:

   ```sh
   python3 -I scripts/bank_transactions.py recover --repo /path/to/repository \
     --proposal /tmp/recovery.json
   ```

Recovery binds the exact marker, external journal and observed canonical files.
Every path must still be its original or intended version, including absence
for additions/deletions. An unknown new file, changed read-set file, changed
journal, missing backup or newer manual edit stops recovery without overwriting
it. The operator must inspect that discrepancy. Recovery acquires the common
lock and checks the reviewed observation again before making changes. It follows
the same router-last and verified-marker-removal protocol in either direction;
recovery itself can be interrupted and inspected again.

A recorded finished outcome cannot be switched to rollback by retry. A recorded
rollback cannot be silently reapplied as the original edit: prepare a new reviewed
operation. If a response is lost after receipt but before marker removal, recover
in the recorded direction to finish cleanup. If the marker is already removed,
replaying the original apply reports its known outcome and removes only its exact
leftover journal. A later correction is preserved and reported with
`still_current: false`, rather than replaced by old memory.

These guarantees require reliable exclusive creation, atomic rename and flush
on one worktree's filesystem. Non-cooperating manual editors can still race a
filesystem check; they should honor visible locks. Distinct worktrees and clones
have separate locks and recoveries. Sharing and reconciliation across them use
the separate Git protocol. Runtime locks, markers, staged files and journals
MUST NOT be committed as shared memory.
