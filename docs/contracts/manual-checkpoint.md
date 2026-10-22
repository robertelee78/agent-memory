# Manual single-file checkpoint and resume

A client with ordinary file access MAY checkpoint an existing routed
`activeContext.md` without the CLI. The result remains readable, versionable
Markdown. It MUST NOT be called an audited admission, a verified mediated
operation, a concurrency-safe transaction or landed shared memory merely
because the editor saved it.

## Before editing

1. The agent MUST select the intended repository, read `memory-bank/MEMORY.md`,
   check its project ID and applicability, and follow its current context route.
   It MUST NOT choose a bank by directory basename or a developer alias.
2. It MUST inspect the bank for `.agent-memory.lock` and
   `.agent-memory-transaction.md`. A visible lock means **do not edit**; wait for
   the writer or inspect bounded lock recovery. A transaction marker means
   **interrupted bank**; stop mutation and route to transaction recovery.
   Age alone MUST NOT justify deleting either file.
3. It MUST reread the complete target immediately before editing, including
   source links and conditions. A client MUST NOT replace it with a stale
   in-session snapshot or a generic template.

This fallback updates one existing routed file. If `activeContext.md` is absent
or explicitly omitted by the router, the agent MUST prepare its useful content
and a separate reviewed initialization/route update. It MUST NOT create an
unrouted placeholder or pretend that a target-plus-router change is one
single-file checkpoint. Multi-file changes need the transaction protocol.

## Checkpoint

The agent SHOULD keep stable headings for current focus, recent meaningful
changes, open questions and the next concrete step. It MUST update only the
intended section, preserving unrelated context, evidence links, distinct
conditions and provenance. A checkpoint MUST distinguish current observations
from approved intent and planned work; it MUST NOT invent an answer, duplicate
the task tracker or paste a raw conversation.

For example, changing the next step SHOULD leave the focus, earlier validated
changes and unresolved question intact:

```markdown
# Active context

## Current focus
Verify a branch-specific lesson; see the linked task.

## Recent changes
The isolated retry test passed; see its recorded evidence.

## Open questions
Whether the lesson applies outside this component is still unverified.

## Next step
Inspect the second clone's refresh result before broadening applicability.
```

The agent MUST save only that file, reread the result, and inspect its diff to
verify that the intended section changed and useful surrounding context survived.
It MUST leave Git HEAD, unrelated index entries and working files alone. It
SHOULD continue through ordinary repository review/commit rules when appropriate.
A local save or feature-branch commit MUST NOT be reported as publication to
main; automatic sharing still needs admission and the memory publication policy.

This manual procedure is cooperative. Checking for a lock and editing later is
not an atomic lock acquisition. A writer or editor ignoring the visible protocol
can intervene; the agent MUST NOT claim the mediated writer's guarantees for a
raw editor operation. Filesystem/repository permissions govern direct access,
not attribution selection or a CLI audit.

## Resume and recovery

A fresh session MAY resume using the router and the applicable current context
alone. It MUST read the saved focus, meaningful changes, unresolved questions,
next step and source/applicability information rather than infer them from a
cache, chat transcript or absent generated index. Plain file tools and ordinary
Git history remain sufficient to inspect and recover the manual edit.

If the readback/diff shows an unintended change, the agent MUST compare its own
edit with the previous version and preserve other people's edits. It MUST NOT
reset the whole repository, discard unrelated staged work or overwrite a newer
file from an old checkpoint. Uncertainty about meaning or provenance calls for
focused clarification; a clean text diff does not settle truth.

## The next mediated write

The next mediated operation MUST reread and validate the external change, resolve
attribution/project identity and apply its semantic/security rules. It MUST NOT
silently overwrite that change from a proposal made before the manual edit.
A fresh proposal MUST bind the newly observed hashes; any new replacement SHOULD
retain compatible manual learning and evidence. Malformed or unsafe external
content MUST trigger inspection/quarantine or clarification through the relevant
operation, not automatic template repair.

The [observed-write helper](observed-writes.md) supplies the mechanical baseline:

```sh
python3 -I scripts/bank_writes.py inspect --repo /path/to/repository
python3 -I scripts/bank_writes.py preview --repo /path/to/repository \
  --path activeContext.md --content reviewed-context.md > proposal.json
python3 -I scripts/bank_writes.py apply --proposal proposal.json
```

These are the same packaged scripts used by the isolated walkthrough, not a
prerequisite for the manual edit itself. Inspect/preview write no bank or runtime
files. Apply of a stale proposal MUST refuse and preserve the manual bytes;
starting a fresh preview after reading valid changes MAY proceed through the
caller’s governed policy. A file-contract validation result alone MUST NOT be
presented as semantic admission or authorization. The product's checkpoint
command adds its dedicated fields and identity gates on this same write kernel.
