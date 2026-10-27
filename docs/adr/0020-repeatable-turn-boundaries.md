---
id: ADR-0020
status: proposed
date: 2026-10-27
amends: [ADR-0004, ADR-0012, ADR-0019]
---

# ADR-0020: Repeatable, quiet memory turn boundaries

## Context

The [RCA](../research/2026-10-27-boundary-reliability-rca.md) identifies three
independent defects: one-prompt-per-turn bookkeeping rejects Codex steering;
quick refresh refuses an unchanged verified import; and Stop does not recognize
proactive completion, making routine success depend on a noisy fallback.
The operator authorized research, independent review, AWA decomposition,
implementation and a signed corrective release. Repo-local Markdown, semantic
comparison, provenance, memory-only publication and ordinary Git rules remain
unchanged. Temporary protocol state is not canonical or personal memory.

## Decision

### T1: Preserve each observed source within an active turn

The Codex ledger MUST give every observed UserPromptSubmit invocation a new
local observation UUID, retaining project, checkout, session, active turn and
immutable prompt digest. This is an observation ID, not a harness-issued message
ID or authentication. Distinct messages under one turn MUST coexist. Identical
text submitted again MUST also create a new observation: an earlier completion
MUST NOT cover a later identical-text approval in a different context. The public
contract exposes no unique submission/retry ID, so registration MUST NOT pretend
to distinguish a transport redelivery from an intentional identical submission.
Extra observations are safely assessed; semantic admission prevents duplicate
canonical learning. Acknowledgment retries for the same observation ID MUST be
idempotent.

Acknowledgment is per observation ID plus exact source digest. Each completed
receipt covers only that observation. Stop succeeds only when every currently
observed ID has its own valid completed receipt; later observations remain
pending. It MUST NOT infer semantic truth or publication from source coverage.

All prompt registration, completion publication and Stop inventory use one
owner-only lock per checkout/session/turn namespace, using nonblocking POSIX
flock on macOS/Linux. Unsafe lock paths, contention or failed validation MUST
preserve state and return concise pending; they MUST NOT steal locks or spin.
Publication verification can occur before the lock, but evidence MUST be checked
again inside it before completion is recorded. Concurrent registration cannot
be erased or covered by an acknowledgment for a different ID.

The v2 namespace MUST be scoped per checkout/session/turn, with at most 128
observations per turn, observation records at most4KiB and completion records at
most64KiB. Both encoded output bounds MUST be checked before exclusive/atomic
publication. Source observations retain identifiers/digests, never prompt text.
Capacity exhaustion MUST retain existing evidence and require inspection, never
truncate or rewrite it. Listing history MUST retain bounded inventory limits.

Existing v1 records remain in place behind a read/acknowledge compatibility shim;
no eager conversion, invented assessment or source reassignment is allowed.
New v2 state uses a separate namespace. Older runtimes cannot establish coverage
of v2 observations; downgrade is not evidence of completion and re-upgrade must
retain pending work. Interrupted/cancelled work stays pending, not no-learning.

### T2: Recognize a verified imported snapshot on the next quick check

Quick refresh MUST recognize an unchanged receipt-covered complete snapshot
only when ALL of the following hold:

- cursor(root) is a verified-local-hint with exact schema/checksum, repository and
  project, actual branch/revision git_context, and valid operation_id;
- the actual transaction receipt at tx.locations(root,bank,operation_id) has
  exact schema, verified:true, outcome:finished, matching operation/project/bank,
  raw receipt hash equal to cursor.receipt_sha256, and resulting_hashes equal to
  cursor.resulting_hashes;
- the COMPLETE current canonical path-to-hash map equals resulting_hashes,
  including path-set equality for additions and deletions;
- semantic.snapshots obtains a new remote observation/fetch during THIS quick
  check, with verified authority history/base and complete bank values satisfying
  base == local == landed, and local hashes equal to the cursor map;
- final canonical source, scope, git context, cursor bytes, index hash, mode,
  boundary and recovery-state pins remain unchanged, and a final remote head
  check agrees with the freshly observed authority.

This establishes byte synchronization only. Semantic truth, relevant recall,
admission and publication remain separate. The result MUST NOT claim that the
importer created every file in the snapshot.

| Existing gate | Decision under the complete conjunction |
| --- | --- |
| Untracked canonical files block quick check | Permit only the receipt-covered complete snapshot; retain every other boundary blocker. |
| HEAD bank must equal current bank | Permit the verified imported snapshot to differ from old HEAD. |
| HEAD/index inventories must equal current bank inventory | Permit their old path sets to differ from the snapshot, but retain complete HEAD-to-index path/blob equality, ordinary100644 modes and stage0. |

Current canonical files retain existing regular-file/mode checks. Staged feature
code remains allowed and untouched. Unknown local changes/additions/deletions,
staged memory, absent/damaged/mixed receipts or cursors, changed branch/revision,
new authority memory, holds, locks, transactions, unsafe modes and any race MUST
retain full-review/refusal. No staging, reset, branch movement or canonical write
may satisfy the quick check. Repeated full no-op refresh retains its usable
completed-snapshot witness without inventing creation provenance. Read-only
optimistic validation is retained; this path adds no writer lock or schema.

### T3: Make compliant completion quiet and keep fallback bounded

Prompt guidance MUST provide a bounded source envelope and route to truthful
assessment before final. Already loaded unchanged instructions need not be
repeated in full. A no-learning turn MUST assess actual observations without
creating a durable record or running a novelty scan for a nonexistent candidate.
Durable candidates MUST still pass the existing semantic admission/publication
gates. A bounded stdin acknowledgment path MAY avoid a temporary input workspace
for no-learning assessments. Stdin is limited to64KiB and MUST refuse published
outcomes; publication requests retain the existing file-based verified path.
File-based compatibility remains available. Encoded completion bounds also apply.

Stop MUST validate completion for the current observed sources and emit no
message or block when all are complete. It MUST NOT interpret missing pending
alone as success, and corrupt/unsafe completed evidence MUST stay pending.
Nullable final-answer text MUST NOT defeat a valid completion. If work remains,
Stop MAY request a short source-bound continuation pointing to the operation
rather than repeat the whole skill. At most one fallback wake per observed
source-set fingerprint is permitted, with a hard maximum of two wakes per active
turn; remaining work stays visibly pending without another block. A generated
continuation digest and source context MUST be retained before emission. Matching
text is only a control-origin hint, not authority to ignore a genuine user source;
any observation remains assessed explicitly. Continuations MUST be bounded and MUST NOT
turn a newer steering source into an already assessed source. Internal generated
instructions MUST NOT be classified as a new operator decision merely because
the harness delivers them as user-role text. Any special recognition MUST use
retained exact generated evidence and context, never an arbitrary text marker.

Native harness tool/hook UI cannot be promised invisible. Routine agent prose
SHOULD omit successful bookkeeping, paths and JSON. Failures, conflicts and
consequential corrections MUST remain visible. Canonical memories remain only
inside their repository bank; private validation receipts remain outside Git.

### T4: Prove repeated normal operation before release

Acceptance MUST test multiple distinct steering submissions in one active turn,
identical redelivery, interruption/resume, late steering racing completion,
legacy evidence, corrupt evidence and nullable final text. It MUST test proactive
completion followed by quiet Stop and a bounded forgotten-operation fallback.

Refresh tests MUST include first real import then quick current, repeated full
no-op then quick current, imported edits/deletions, post-import local edits and
new files, stale or damaged receipt/cursor, branch/index changes, fresh authority
changes and retained source/mode/lock/race guards. Code/index/branch preservation
and no duplicate memory MUST be verified.

A bounded real Codex run MUST exercise distinct and identical-text messages and
at least two normal completion cycles. Each cycle MUST show all observed sources
retained, truthful assessments, completed matching receipts and a quiet Stop.
A real import followed by quick synchronization MUST succeed without full review
repetition. Publication and recall are verified separately if durable learning is
admitted; no-learning MUST NOT claim either. Source mismatch on ordinary steering,
missing-source coverage, forced routine fallback or repeated unchanged-bank review
fails acceptance. Synthetic governance/freshness limits MUST remain explicit. Claude/OpenCode common-memory regressions MUST pass;
this release does not claim live-model coverage of unchanged adapters unless
actually observed. Tests MUST run locally on macOS and Linux, with each command
bounded to at most 10–15 minutes. Signed public fresh-install/update/no-op and
consumer-memory isolation checks MUST pass. Actual production activation and any
remaining pending legacy observations MUST be reported accurately.

## Alternatives and delivery

Overwriting a prior prompt hash, auto-acknowledging every turn, staging imported
memory to satisfy a guard, and hiding consequential failures are rejected.
One successful demonstration is insufficient repeated-operation evidence.

This corrective scope targets v0.0.8 with its own public AWA Project. Unfinished
v0.0.7 roadmap work remains unchanged; no delivery or closure is inferred for it.
Independent design review precedes task readiness. Implementation acceptance,
signed publication and release delivery require separate evidence.
