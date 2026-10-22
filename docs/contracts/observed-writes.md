# Observed single-file writes

The portable write kernel gives all bank mutation operations one worktree-local
exclusive lock, observed-byte checks, atomic per-file replacement and readback.
It is a mechanism beneath admission, identity binding and semantic review. It
MUST NOT decide that arbitrary Markdown is true, safe, admitted or authorized.
Governed callers MUST first resolve the developer/project and complete the
operation's policy checks. The first-release integration and skill guides use
this kernel; raw OS file access remains possible under repository permissions.

The current public primitive replaces one existing routed Markdown target. It
MUST NOT delete content, change the router/project ID, initialize an empty bank
or present several replacements as one atomic transaction. Those operations use
the dependent initialization/binding/multi-file and forgetting protocols.
The [common scope resolver](scope-binding.md) now guards public mediated writes.
Checkpoint's product-specific fields remain a separate caller.

```sh
python3 -I scripts/bank_writes.py inspect --repo /path/to/repository
python3 -I scripts/bank_writes.py preview --repo /path/to/repository \
  --path activeContext.md --content reviewed-context.md > proposal.json
python3 -I scripts/bank_writes.py apply --proposal proposal.json
```

The copied artifact has the same `scripts/bank_writes.py` entry point. Commands
emit one JSON document; exit 0 means the command completed, exit 1 means refusal
or an interrupted/uncertain result requiring inspection, exit 2 means invalid
arguments. A preview binds a random operation ID, stable bank project ID,
canonical repository root, complete observed Markdown hashes, target path and
reviewed replacement bytes, plus a pinned resolved `scope_context` when setup
is complete. A missing context or a changed developer/project registry refuses
apply and requires a fresh preview. Identity is checked again under the bank
lock. Preview writes nothing. Inspect validates the current
file contract and returns the canonical Markdown path inventory; it does not
claim admission or current truth for each prose statement. A visible lock or
transaction marker MUST prevent a stable read result and route to retry/recovery.

## Lock, observation and replacement

Apply MUST acquire `memory-bank/.agent-memory.lock` through exclusive creation
**before inventory**, then hold it through bank readback and receipt creation.
Its metadata names the operation ID, host, PID, operation and qualified start
time. A second cooperating writer MUST stop. Read-only commands MUST NOT acquire
write locks or silently repair files. Identical bank IDs in different worktrees
MUST NOT imply a cross-machine or cross-worktree filesystem lock.

After acquiring the lock, the writer inventories and validates the bank, then
compares its hashes and project ID with the proposal. It stages a same-directory
temporary file, flushes its bytes and rechecks the complete observed bank
immediately before replacing the target. An external target or read-set change
MUST stop the operation and preserve that change. After the atomic replacement,
it flushes the parent directory, validates and hashes the bank again, then
creates and reads back a private operation receipt before releasing the lock.
Temporary staging files are removed after ordinary errors.

A normal replacement changes one target, preserving the router, other context,
Git HEAD, index and unrelated working files. The kernel MUST NOT run Git merges,
rebases, commits, staging or credential operations. Higher-level sharing owns
those decisions and its additional Git-state checks.

A fresh proposal MAY incorporate a valid manual edit after the agent reads and
validates it. A stale proposal MUST NOT overwrite that edit. File validation is
the baseline; typed records, admission, applicability, security and semantic
meaning remain their respective callers' responsibilities.

## Receipts and replay

Receipts live outside the repository under
`${AGENT_MEMORY_STATE_HOME:-${XDG_STATE_HOME:-$HOME/.local/state}/agent-memory}`
in `writes/<canonical-bank-path-sha256>/operations/<random-operation-id>.json`.
The bank-path key separates worktrees even when their project ID matches.
State/config paths MUST be absolute and canonical without symlinks; runtime
state inside the repository MUST be refused. Unrelated state files and
colliding operation IDs MUST be preserved, not overwritten.

The receipt records the operation/project ID, request digest, observed/resulting
hashes, changed memory paths and readback time. It contains no replacement
Markdown body. These local hashes are operational metadata, not content-free
durable deletion tombstones. Forgetting/purge callers MUST account for retained
local metadata when determining their bounded erasure scope.

`verified: true` proves a successful readback or a recognized earlier operation.
The response's `writes_performed` describes this invocation's durable bank
replacement, excluding its temporary lock/receipt bookkeeping. `still_current`
separately reports whether the bank still matches the recorded result. A replay
MUST NOT reset a later correction merely to recreate an older result.
Higher-level callers MUST inspect `still_current` before treating it as the
current bank or advancing a refresh cursor.

If a process exits after replacement but before receiving or storing its
receipt, the target can already contain the intended bytes. After bounded lock
recovery, rerunning the **same proposal** MUST inventory reality. If the complete
bank matches the intended result, it verifies that present result and records
`outcome: already-present` without replacing memory again; it does not invent
proof of which process performed the earlier write. If the bank matches neither
the observed base nor intended result, it MUST stop for reconciliation. An
existing known receipt permits read-only replay with `already_applied: true`;
a reused ID with different proposal bytes MUST fail. Losing disposable receipts
MUST NOT destroy canonical memory, though it can require reconciliation.

A failure before replacement and a lost response after replacement are distinct.
After any apply failure, the caller MUST inspect/replay rather than assume that
nothing changed. Disk-full, permission or external-edit failures after replacement
can leave a changed readable target without a success receipt. The helper MUST
return failure instead of claiming verification, and MUST NOT overwrite a newer
external edit while attempting rollback.

## Stale locks and filesystem limits

```sh
python3 -I scripts/bank_writes.py recover --repo /path/to/repository > recovery.json
python3 -I scripts/bank_writes.py recover --repo /path/to/repository \
  --proposal recovery.json
```

Recovery MUST recognize the exact lock metadata, prove its same-host PID is
gone, acquire an advisory lock on its open descriptor, and compare inode/bytes
with the reviewed proposal immediately before removal. A live owner, another
writer/recovery, unknown lock, changed proposal or unsupported locking MUST stop.
A foreign-host lock requires explicit operator review; this helper MUST NOT
force-remove it based on age. Recovery MUST hold the descriptor through removal
so a competing recovery cannot remove a newly created writer lock. It only
recovers the lock; a transaction marker still requires transaction recovery.

The baseline requires a local filesystem with reliable exclusive create,
same-filesystem atomic rename, file/directory flush and permissions. Automatic
stale recovery additionally requires supported advisory file locking. A network
share or filesystem lacking those semantics MUST NOT be advertised as safe for
concurrent mediated mutation. Tests exercise the actual supported primitives;
they do not certify every filesystem. The final recheck and rename still have a
small interval in which a non-cooperating manual editor can write. Such editors
SHOULD honor the visible lock; the kernel cannot promise isolation from mutually
distrusting processes sharing an OS account or editors ignoring its protocol.

## Runtime and shared Git paths

The canonical path inventory includes only validated project Markdown. Lock
files, `.agent-memory-write-*.tmp`, transaction markers, recovery journals,
receipts and caches MUST NOT enter memory publication. Staging files use a
reserved `.agent-memory` prefix; journals/receipts stay outside the repository.
A transaction marker stops inspection instead of becoming another history file.
Sharing callers MUST use this path policy plus their own admission/security
checks, not `git add .` or an unrestricted directory snapshot. The publication
story separately proves the full Git path/ancestry restrictions.

The isolated suite invokes the real public commands with competing processes,
manual edits while a writer is paused, crashes before/after replacement and
receipt creation, and separate worktrees. Explicit failpoints require a verified
disposable test-world token and MUST NOT be enabled by memory or proposal data.
