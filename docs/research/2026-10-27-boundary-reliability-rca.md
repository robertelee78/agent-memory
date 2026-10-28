# Repeated-turn memory reliability RCA

## 1. Steering is incorrectly treated as source corruption

The actual Codex session transcript contains multiple distinct user submissions
under the same active turn ID. The private pending ledger preserves the digest
of its first submission; subsequent submissions cause boundary.source_mismatch.
The transcript was inspected read-only; raw user content and individual hashes
are deliberately not copied into this public research artifact.

codex_prompt.py keys one immutable record by session+turn. It assumes turn_id is
a unique prompt identity. The public hook contract describes it as the active
turn ID. Codex supports steering while work is running. The guard correctly
refuses to overwrite evidence, but its model cannot represent normal input.
Existing tests reinforce the mistake: changed text under one turn is tested only
as a tamper case, not as a legitimate additional submission. A single successful
live cycle therefore did not establish multi-message daily reliability.

## 2. Quick refresh rejects the result of a successful import

Actual root refresh e043d5b2-1cf8-4c06-8608-e5711b3e74fb imported reviewed memory
from authority 711df384ab4420c5bb0347e6b990558d86e5c47a. It verified relevant
imported recall and preserved feature HEAD, index and unrelated files. The next
refresh current refused canonical-memory-untracked. Disposable reproduction
confirmed: first import succeeds, immediate quick current refuses, another full
refresh succeeds with no canonical writes. Full refresh is not broken; its quick
check repeatedly demands review of an unchanged reviewed result.

Three quick-check gates assume committed-bank equality: the local boundary
rejects untracked canonical files, HEAD's bank must equal local, and the HEAD/index
inventory must equal local. Those assumptions conflict with the refresh contract
that deliberately leaves feature HEAD/index unchanged. Existing tests checked a
second full refresh but omitted immediate quick current after a real import.

A cursor's resulting hashes cover the reviewed complete snapshot, not creation
ownership of every included file. Preserved local learning can also appear in a
result. Any correction must name receipt-covered state accurately and must not
infer importer creation merely from resulting_hashes.

## 3. Normal completion exposes fallback internals

UserPromptSubmit guidance supplies a long generic workflow, without a convenient exact
source envelope/proactive completion route. Stop always opens pending and never
recognizes an existing completed receipt. A valid proactive acknowledgment removes
pending, after which Stop can incorrectly report boundary.missing. Consequently
the successful path relies on blocking a final reply and sending the entire
workflow back as a visible continuation. Codex owns the native blocked UI, but
our long reason and compulsory fallback make ordinary operation unnecessarily
noisy. Temporary assessment workspaces are protocol state outside Git, not a
second canonical memory bank; showing them without context caused confusion.

Stop additionally rejects nullable/empty last_assistant_message merely to compute
a final-answer hash which completion verification never consumes. The public
contract allows null. New work must test completed receipts before deciding that
final-response prose is needed.

## Sources and limits

- Official hooks: https://learn.chatgpt.com/docs/hooks
- Official Codex steering description:
  https://developers.openai.com/blog/mastering-codex-remote-for-engineering
- Implemented v0.0.7: codex_prompt.py, codex_stop.py, codex_boundary.py,
  memory_refresh.py, sharing_boundary.py and their contract tests.
- Read-only actual session metadata, root refresh receipt and disposable repeat
  reproduction reviewed by separate research agents.

These findings do not invalidate the v0.0.7 checkout-identity repair. They expose
missing behavior in steering, the quick path after import and ordinary quiet
completion. Canonical memory remains repository-local. No old source digest or
completion outcome may be rewritten to hide these failures.

## Native-adapter extension after operator clarification

The operator explicitly requires good behavior in OpenCode, Codex and Claude.
Claude2.1.306's current UserPromptSubmit adapter emits generic guidance without
reserving source evidence. First AM persistence occurs in claude_stop.py after
claude_source.source requires a visible transcript entry and final assistant
text. An interruption before Stop can therefore leave no AM reservation, and
proactive completion before first Stop has no pending record to acknowledge.
The official Claude hook reference says transcript writes can lag hook execution;
pre-prompt registration must use direct event session/prompt identifiers and
prompt digest, then verify the transcript-entry UUID later when needed.
https://code.claude.com/docs/en/hooks#common-input-fields

Claude already checks a completed receipt at Stop; that differs from Codex.
Its existing pending marker prevents repeated registration but does not solve
pre-prompt source enrollment or transcript-lag behavior. A tools-restricted
review invocation returned only memory-pending prose instead of its requested
verdict, motivating explicit task-answer preservation in native continuations.

OpenCode2.1.3 uses native message IDs and a synthetic queued pass. Its existing
intent records retain hashed keys and repository metadata, not original native
session/source IDs, so they cannot independently enumerate original sources
across sessions. Current recovery searches the original native session and
requires a completed final assistant record; interrupted windows intentionally
remain pending/unavailable. Failed queue attempts remain pending instead of
retrying in a loop. These are qualified recovery limits, not Codex's turn/hash
collision. New bookkeeping must preserve native IDs and expose bounded recovery
routes without inventing missing source or replacing the native callback model.

No claim is made that old interrupted sources can be retrospectively marked
assessed. The live acceptance matrix must verify repeated native cycles, source
retention, bounded pending behavior and preservation of substantive task answers.

## Live qualification findings and design refinement

Real Claude Code 2.1.306 streaming input delivered three UserPromptSubmit
callbacks (different and identical queued follow-ups) with one active prompt_id.
The shared observation protocol correctly retained three IDs. The additional
Claude-only transcript gate then refused the last two: the native log retained
only the first input as a plain user row and the absorbed follow-ups as queue
removals with commandUuid/deliveryId but no promptId. These are not proven
callback-to-transcript bindings. Inferring a mapping from text/order would
introduce another unsupported identity assumption.

ADR-0020 now uses the actual prompt callback as the v2 source witness consistently
with Codex. Explicit assessments remain required, exact observed digests and
contexts remain checked, and recovery requires the actual original source and
context. Legacy transcript-UUID records are unchanged. Opus and independent
review accepted this narrow refinement; previous failed live results are retained
rather than recast as successful tests.

Other live checks exposed two presentation/protocol defects: the old legacy
example appeared before the v2 fields, causing a model to send source_uuid for an
observation; and an async Claude fallback could leave its SDK final result empty
after the answer was already emitted. The current protocol now appears first,
required fields and repair instructions are explicit, and continuations require
the substantive answer in their final response. Production asyncRewake normal
and no-tools tests separately verify those corrections.
