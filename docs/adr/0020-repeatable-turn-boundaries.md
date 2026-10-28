---
id: ADR-0020
status: accepted
date: 2026-10-27
amended: 2026-10-27
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

Accepted for v0.0.8 execution under the operator’s explicit pipeline instruction,
Opus design acceptance and independent code-aware acceptance. This is not
implementation acceptance or release evidence.

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

Prompt registration MUST be append-only and independent of the completion/
inventory lock. Each invocation reserves one of at most128 fixed source slots
with exclusive creation, writes its immutable observation and verifies durable
readback before reporting registration success. A reserved partial/invalid slot
counts as pending; inventory MUST NOT ignore it. Thus registration contending
with completion or Stop still leaves discoverable source evidence. If safe
durable registration is impossible, UserPromptSubmit MUST block that input with
a short actionable error, rather than pass unrecorded input as tracked.

Completion recording and Stop inventory use one owner-only lock per checkout/
session/turn namespace, using nonblocking POSIX flock on macOS/Linux. Unsafe
lock paths or contention preserve state and return concise pending; they MUST
NOT steal locks or spin. Publication verification can occur before the lock,
but evidence MUST be checked again before completion is recorded. Stop MUST
repeat its source inventory/pins before a quiet result, so a concurrently
registered source cannot be credited by an older acknowledgment.

The v2 namespace MUST be scoped per checkout/session/turn, with at most 128
observations per turn, observation records at most4KiB and completion records at
most64KiB. Both encoded output bounds MUST be checked before exclusive/atomic
publication. Source observations retain identifiers/digests, never prompt text.
Capacity exhaustion or failed registration MUST leave a bounded per-turn pending
fault indication whenever safe state access permits, so completion of only the
previously recorded observations cannot conceal an omitted input. A fault MUST
NOT be silently cleared or called assessed. Existing evidence is retained for
inspection, never truncated or rewritten. Listing history retains bounded limits.

A bounded checkout-level pending-turn index MUST retain references to turns
with unresolved observations, reservations or faults across sessions. It holds
at most4096 turn references and no prompt text. Registration MUST establish a
durable index reference before accepting an input and recheck it after source
publication. Completion/index cleanup MUST recheck inventory after removing a
reference and restore it if work remains; index maintenance MUST NOT orphan a
concurrently appended source. Index capacity or unsafe state blocks acceptance
with an actionable error; it cannot silently evict an older turn.

At prompt time the adapter MUST expose a bounded prior-pending summary, with a
route to the actual retained original identifiers. It SHOULD avoid repeating an
unchanged notice within one session. Stop reports earlier pending work concisely
without blocking unrelated current-turn completion. A later session may assess
an earlier observation only after recovering its actual original source and
verifying its digest; current input is never substituted or reassigned.

Existing v1 records remain in place behind a read/acknowledge compatibility shim;
no eager conversion, invented assessment or source reassignment is allowed.
Legacy pending observations MUST participate in the bounded backlog view, even
if they are not in the v2 index. New v2 state uses a separate namespace. Older
runtimes cannot establish coverage of v2 observations; downgrade is not evidence
of completion and re-upgrade must retain pending work. Interrupted/cancelled work
stays pending, not no-learning.

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

Codex prompt guidance MUST provide a bounded source envelope and route to truthful
assessment before final. Already loaded unchanged instructions need not be
repeated in full. A no-learning turn MUST assess actual observations without
creating a durable record or running a novelty scan for a nonexistent candidate.
Durable candidates MUST still pass the existing semantic admission/publication
gates. A bounded stdin acknowledgment path MAY avoid a temporary input workspace
for no-learning assessments. Stdin is limited to64KiB and MUST refuse published
outcomes; publication requests retain the existing file-based verified path.
File-based compatibility remains available. Encoded completion bounds also apply.

Codex Stop MUST validate completion for the current observed sources and emit no
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

Claude and OpenCode MUST use their supported native lifecycle and original
message/source identifiers (including verified transcript-entry UUIDs, or an
explicit local observation ID when the native API does not expose a message ID).
Accepted native inputs MUST be retained before their first completion callback,
so interruption cannot erase source tracking. Claude enrollment MUST NOT demand
a transcript row before the native hook has durably published it. OpenCode
backlog metadata MUST retain original native identifiers for later discovery;
unavailable original evidence stays pending, not automatically assessed.
They MUST NOT inherit Codex turn IDs or pretend a
shared callback API exists. Their guidance and continuations MUST be shortened
to bounded operation routes and native source context, rather than repeating the
full workflow. A native post-answer pass MAY be used when supported; it MUST
remain source-bound and leave unavailable evidence pending.

The bounded wake rule applies to ALL THREE adapters, using each native session/
message lifecycle: at most one wake per native source-set fingerprint and at
most two wakes per native lifecycle. The attempt count MUST be retained durably
before each wake/queue submission. At the cap, remaining work stays visibly
pending and the adapter stops blocking or queueing another continuation. Native
re-entry flags alone are not proof that this durable limit was enforced.

All three adapters' guidance MUST preserve the substantive operator-facing task
answer across a memory continuation. Routine completion MUST NOT replace a code
answer or review verdict with bookkeeping. If tools are unavailable, preserve the
original answer and add only a concise pending qualification when consequential;
never claim an assessment or keep waking indefinitely. Each generated guidance/
fallback message MUST remain at most2048 UTF-8 bytes. Bounded source-window tools
retain their existing separate evidence limits and truncation qualifications.

Native harness tool/hook UI cannot be promised invisible. Routine agent prose
SHOULD omit successful bookkeeping, paths and JSON. Failures, conflicts and
consequential corrections MUST remain visible. Canonical memories remain only
inside their repository bank; private validation receipts remain outside Git.

### T4: Prove repeated normal operation before release

Acceptance MUST test multiple distinct steering submissions in one active turn,
identical-text submissions, interruption/resume, registration contending with
completion and Stop, late steering racing completion,
legacy evidence, corrupt evidence and nullable final text. Cap exhaustion then
a new turn, and interruption then a new session, MUST expose prior pending work
without falsely completing it or blocking unrelated work. It MUST test proactive
completion followed by quiet Stop and a bounded forgotten-operation fallback.

Refresh tests MUST include first real import then quick current, repeated full
no-op then quick current, imported edits/deletions, post-import local edits and
new files, stale or damaged receipt/cursor, branch/index changes, fresh authority
changes and retained source/mode/lock/race guards. Code/index/branch preservation
and no duplicate memory MUST be verified.

Live qualification is REQUIRED for OpenCode, Codex and Claude, using the actual
supported installed clients (OpenCode2.1.3 at the operator's known path). Each
MUST complete at least two ordinary turns with native source tracking, truthful
assessment/completion, preserved task answers and no repeated fallback loop.
Test distinct and identical-text follow-ups using each native client's supported
steering or queue behavior; do not relabel queued work as same-turn steering.
Interruption/resume and pending recovery MUST retain original source attribution.
Codex MUST additionally demonstrate multiple actual submissions in one active
turn and quiet proactive completion followed by Stop. Claude/OpenCode MUST
show their actual native continuation/completion behavior and bounded messages.
A no-tools case MUST preserve the substantive task answer and remain pending
without a loop. Test capabilities and any unverified limitation explicitly.

A real import followed by quick synchronization MUST succeed without full review
repetition. Publication and recall are verified separately if durable learning is
admitted; no-learning MUST NOT claim either. Source mismatch on ordinary steering,
missing-source coverage, overwritten task answers, unbounded routine fallback or
repeated unchanged-bank review fails acceptance. Synthetic governance/freshness
limits MUST remain explicit. Local macOS/Linux contract checks and signed public
fresh-install/update/no-op plus consumer-memory isolation checks MUST pass. Each
command MUST remain bounded to at most10–15minutes.

Actual production activation and any
remaining pending legacy observations MUST be reported accurately.

## Native callback evidence refinement

For newly enrolled Claude observations, the direct UserPromptSubmit payload is
source evidence. Native session/prompt IDs correlate a lifecycle; they MUST NOT
be treated as unique submission or transcript-entry identities. Every callback
MUST retain its own local observation UUID and exact prompt digest, including
identical text under the same prompt ID. Completion MUST bind that exact
observation, context and digest. It MUST NOT require a later transcript user row
or infer a native message UUID from queue content, ordering or callback ordinal.
The v2 observation and request formats MUST reject `source_uuid`; the v1 shim
MUST retain its existing native transcript-UUID requirements without relabeling
old records. The retained native-source hint is lifecycle correlation only.

Native transcripts and queue records MAY assist source recovery; a private
reference is only a locator. For earlier or interrupted work the agent MUST
recover the actual original source, independently check its digest and assess its
actual context before acknowledgment. Unknown, unavailable or ambiguous original
sources MUST remain pending. Matching identifiers or hashes, callback existence
and injected instructions MUST NOT imply assessment or completion. No-learning
and source-to-publication judgments remain agent assertions; independent
admission and authority-publication gates remain unchanged. This mirrors the
native-callback witness already used by Codex, without inventing transcript IDs.

Tests MUST cover three actual native callbacks sharing one prompt ID, with
distinct and identical text, separate explicit completions and quiet Stop only
when all observations are complete. Wrong digests, unknown observations and
cross-context acknowledgments MUST remain refused. Interrupted or ambiguous
recovery MUST remain pending unless exact original evidence is recovered.

## Alternatives and delivery

Overwriting a prior prompt hash, auto-acknowledging every turn, staging imported
memory to satisfy a guard, and hiding consequential failures are rejected.
One successful demonstration is insufficient repeated-operation evidence.

This corrective scope targets v0.0.8 with its own public AWA Project. Unfinished
v0.0.7 roadmap work remains unchanged; no delivery or closure is inferred for it.
Independent design review precedes task readiness. Implementation acceptance,
signed publication and release delivery require separate evidence.
