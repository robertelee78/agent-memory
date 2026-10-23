# Observed bank writes and replay

A session MUST use the common [identity](../identity/GUIDE.md) resolver and
complete the operation's admission, applicability and
[security](../security/GUIDE.md) checks before a mediated mutation. The public
write primitives provide mechanics, not semantic admission. Changing an
operation label MUST NOT substitute for a governed capture/correction/forget
decision or permission to publish memory.

A governed durable-learning writer MUST follow
[comparison](../comparison/GUIDE.md) before preparing its delta and revalidate
the source-bound judgment against its locked bank. Before landing, it MUST repeat
the comparison against actual latest shared memory. A stale or unavailable
comparison MUST NOT count as proof that no previous learning exists.

The [checkpoint](../checkpoint/GUIDE.md), [initializer](../initialize/GUIDE.md)
and explicit identity-binding operations own their product-specific inputs.
For an independently reviewed existing-file replacement a session MAY use:

```sh
python3 -I "$SKILL_DIR/scripts/bank_writes.py" inspect --repo /path/to/repo
python3 -I "$SKILL_DIR/scripts/bank_writes.py" preview --repo /path/to/repo \
  --path activeContext.md --content /private/path/reviewed.md > /private/path/proposal.json
python3 -I "$SKILL_DIR/scripts/bank_writes.py" apply --proposal /private/path/proposal.json
```

Preview writes nothing and binds complete observed Markdown hashes, project,
selected identity, exact replacement bytes and a random operation ID. Apply
MUST hold the exclusive per-bank lock from inventory through readback, refuse
changed observations, replace only the selected existing target atomically,
and verify a private receipt before reporting success. It MUST preserve other
memory, unrelated code, index bytes and HEAD. The lock coordinates cooperating
processes in that worktree; it is not a cross-machine/global lock or protection
from a manual editor ignoring the protocol.

## Several canonical files

Adding/removing routes or records requires the recoverable multi-file protocol:

```sh
python3 -I "$SKILL_DIR/scripts/bank_transactions.py" preview --repo /path/to/repo \
  --files /private/path/complete-reviewed-files.json --operation replace > /private/path/transaction.json
python3 -I "$SKILL_DIR/scripts/bank_transactions.py" apply --proposal /private/path/transaction.json
```

The files input is the complete canonical bank-relative Markdown map, including
unchanged files. A session MUST preserve their observed bytes, conditions,
evidence and provenance. The writer rechecks the whole bank under the lock,
stores private base/intended recovery bytes, and exposes a content-free marker
before sequential atomic file replacements and router-last readback. It MUST
NOT claim global filesystem atomicity. Proposals/journals containing memory
bytes MUST remain private and outside shared commits.

## Interrupted or uncertain outcomes

A failure after replacement can leave saved memory without a success receipt.
A session MUST inspect and retry the **same original proposal**, keeping its
operation ID. Reusing an ID with different bytes MUST fail. Recognized replay
MUST NOT rewrite a later correction. `verified: true` proves that operation's
readback; `still_current` separately indicates whether its recorded result is
still the present bank. A caller MUST check currentness before using it as a
refresh/publication cursor or fresh projection source.

A visible lock requires explicit lock recovery before bank resolution:

```sh
python3 -I "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo > /private/path/unlock.json
python3 -I "$SKILL_DIR/scripts/bank_writes.py" recover --repo /path/to/repo \
  --proposal /private/path/unlock.json
```

Unknown, live or foreign-host ownership MUST stop automatic cleanup; age is
insufficient. An interrupted transaction additionally requires a reviewed
finish/rollback proposal from `bank_transactions.py recover --repo ... --mode
finish` (or `rollback`) and apply with `recover --repo ... --proposal FILE`.
Initialization finish MUST revalidate original source evidence; identity-change
transactions MUST resume through their original binding bundle. A session MUST
preserve unknown markers or missing journals for explicit inspection.

Private write/recovery receipts are separate from optional historical events.
There is no optional historical-event writer in this baseline. Any future
writer MUST use the stable event ID as its idempotency key and refuse that ID
with different content. Historical events/projections MUST NOT replace the
current bank, and terminal deletion MUST use only its content-free tombstone.
Any owned view/cache/index MUST honor current invalidation before rebuild.

Exit 0 means the command completed; apply still requires its verified/current
receipt. Exit 1 means refusal or uncertain outcome requiring inspection, and
exit 2 means invalid arguments. Ordinary memory edits remain local/pending
sharing until actual publication and refresh evidence establishes visibility.
