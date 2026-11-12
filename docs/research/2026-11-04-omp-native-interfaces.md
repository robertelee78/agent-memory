# omp native interfaces: source-bound research for #249

This is interface research, **not an implementation or native qualification**.
The approved [execution design](https://github.com/robertelee78/agent-memory/blob/e78eee13e4f4533d2a8871184a1dbb7e6f0aaa13/docs/design/v0.0.15-omp-integration.md)
is the normative integration contract. The local executable observed on 2026-11-04
reported `omp/18.16.0` and SHA-256
`d4a10d97488d6054d2b18198bd24f461079952c7e0fb75ba66336ca0df0cb99e`.
The source reviewed below is pinned to upstream commit
[`650f3a108229ee11bf68ba3377f4e8c3391cd424`](https://github.com/can1357/oh-my-pi/tree/650f3a108229ee11bf68ba3377f4e8c3391cd424);
source-to-binary equivalence has not been established. No other version is
qualified by this inspection.

## Discovery and activation

[Skills](https://github.com/can1357/oh-my-pi/blob/650f3a108229ee11bf68ba3377f4e8c3391cd424/docs/skills.md)
are discovered one level below a skills root, expose metadata to the model when
a suitable read tool is active, and are read on demand via `skill://`. Native
skills require a description. Global disabling, source filters, and same-name
collisions affect discovery; disk presence is not evidence of model visibility
or invocation. The explicit `/skill:<name>` interactive command and direct
`skill://` reads are distinct from automatic boundary work.

[Extension loading](https://github.com/can1357/oh-my-pi/blob/650f3a108229ee11bf68ba3377f4e8c3391cd424/docs/extension-loading.md)
scans `<cwd>/.omp/extensions` and the active agent directory's `extensions/`;
profile and environment selection affect that directory. Native scanning accepts
`.ts` and `.js`. `--no-extensions`, disabled-extension filters, explicit paths,
and runtime suspension have distinct effects. An extension file existing on disk
does not establish that it was discovered or that callbacks ran. Factory module
state is shared across rebound child sessions, so a future observer must isolate
per-session state and use `ctx.agent.kind`, not depth, to exclude subagents and
advisor activity. This project must not infer its owned target from the current
profile or rewrite global permissions and settings.

## Events, privacy and attribution

The pinned [extension contract](https://github.com/can1357/oh-my-pi/blob/650f3a108229ee11bf68ba3377f4e8c3391cd424/docs/extensions.md#prompt-and-turn-lifecycle)
describes `input` as submission ingress, not a reliable complete record of
user-role messages; print and ACP are outside that interception contract.
`before_agent_start` runs on actual dequeue of user-containing prompt/steering
batches and may be retried. Hidden agent-attributed companions do not appear in
its prompt field even when delivered in the batch. Therefore neither one input
callback nor that prompt field proves complete native source coverage.

`message_end` is a notification, not a persistence barrier. In the pinned
[agent session implementation](https://github.com/can1357/oh-my-pi/blob/650f3a108229ee11bf68ba3377f4e8c3391cd424/packages/coding-agent/src/session/agent-session.ts#L4609-L4634),
assistant display events may be deobfuscated while the original persistence
path retains placeholders. Extension callbacks must not retain or hash display
payloads. The native stored representation is also not a universal secret
redactor: user and tool material may contain plaintext secrets missed by shared
screening. Snapshot retention and privacy must be disclosed.

`ctx.sessionManager.getBranch()` exposes native stored entries; the documented
[entry taxonomy](https://github.com/can1357/oh-my-pi/blob/650f3a108229ee11bf68ba3377f4e8c3391cd424/docs/extensions.md#session-entry-roles-messagerole-is-camelcase)
uses `toolResult`, not `tool_result`, and pairs its `toolCallId` with a
`toolCall` block in an assistant entry. `custom_message`, compaction and branch
summaries are entry types, not ordinary native message roles. Resume, fork,
branch, rewind, compaction and missing persistence cannot be flattened into a
new human admission or presumed complete coverage. In particular, events may
be in flight while persistence is still pending; a missing stored source must
remain a gap rather than an invented digest.

`session_stop` is an awaited main-session advisory hook with at most eight
native advisory continuations; it is not a guarantee that an agent executes a
requested action. Blocking is a separate, potentially indefinite capability
and is not permitted by the approved observer contract. Native tool approval
can short-circuit before `tool_call`; denied calls do not execute or emit a
normal `tool_result`. No callback may bypass that boundary by using `pi.exec`
for canonical work. `sendMessage` next-turn and follow-up delivery can be
queued or deferred; they do not prove execution or completion.

## Candidate extension points for ADR-0023 D2

These are source-level options, **not implemented graph-backed cues**:

* `before_agent_start` can provide bounded advisory context at prompt or
  user-containing steering/follow-up preparation. Its retries, joined prompts,
  hidden companions and cancellation mean it cannot itself certify source
  identity or coverage. Guidance must preserve original task/user input and
  remain non-authoritative.
* `tool_result` can return passive `additionalContext` without rewriting tool
  output; handlers compose in registration order. It is unavailable for calls
  denied before execution and cannot establish that the agent acted on a cue.
  An extension must not change result content, permission policy, model or
  tools merely to force recall.
* An explicit ordinary model tool call can invoke the shared skill/CLI for
  agent-controlled progressive recall, retaining normal tool permission and
  source/current-bank checks. The event hook is not an independent memory
  reader or admission engine.
* `session_stop` permits a bounded advisory continuation for pending work, not
  a durable completion receipt. The agent may refuse, be interrupted, omit the
  work, or encounter an unavailable authority. Persist original source and
  show pending rather than inferring success.

No interface above establishes cue quality, source-publication causation,
model selection, graph availability, or successful delivery. ADR-0023 must
separately validate any future cue policy against native batches, cancellation,
permissions, and actual client versions.

## Qualification boundary

[RPC](https://github.com/can1357/oh-my-pi/blob/650f3a108229ee11bf68ba3377f4e8c3391cd424/docs/rpc.md)
exposes prompt, steer, follow-up, abort, session transition and branch commands,
with responses correlated by ID. Its streamed events and protocol framing are
not an independent durable transcript or proof of canonical memory. A future
qualification must bind the actual executable hash, installed owned extension,
real provider and source candidate; inspect native and bank readback separately.
It must exercise multi-turn recall, capture, search-before-write, refresh and
peer-clone exchange, plus refusals, cancellation, actual offline authority,
retry, concurrent sessions, recovery and install/update/doctor/uninstall.
The approved design also requires the detailed batch, capacity, mixed-version,
macOS/Linux and Codex/Claude/OpenCode regression matrix. None of those cells is
established by this research or by `omp --version`.
