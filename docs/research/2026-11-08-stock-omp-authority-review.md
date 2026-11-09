# Stock omp compatibility and stored-source authority

Status: operator constraint recorded; independent Opus policy/task-coverage review passed. This is not implementation, native qualification or release acceptance.

## Decision

AM integrates with stock, independently updated omp. It does not maintain a fork,
require a custom build/upstream change, reorder or disable operator extensions,
weaken permissions or silently replace the client installation.

Authoritative clarification: [ADR-0004 at 8c6811c](https://github.com/robertelee78/agent-memory/blob/8c6811c9fce53df0a956901d6e032ba31b784b50/docs/adr/0004-client-independent-integration.md),
Git blob `645c991891cb9ed05ed02000ff3df013e468c881`.

## Why

The actual stock 18.16 RPC race delayed an old-session notification across a
successful `new_session`. AM then invented a genuine-user root in the new
session, whose stored history contained no genuine user. The old checked
before-switch view had already captured the real old user correctly.
[Curated actual evidence](data/2026-11-06-omp-native/dispatch-race-20261107.json)
retains exact native/source identities and seven terminal helper results: six
verified, one failed/pending. No extra wake or full source-coverage pass is claimed.

The fault is AM's use of notification metadata as ownership authority. An
upstream event-epoch change is not the only solution and is not an AM dependency.

## What

Callbacks are scheduling hints. A checked native stored-user entry and its actual
identity establish human origin; callback role, timing, position and mutable
context cannot establish ownership or resolve a real source gap. Missing
notification mapping remains distinct from genuinely missing learning source.
Existing safe sources remain assessable; real gaps and immutable evidence are
not waived or rewritten.

The detailed AM-only capture/transport/recovery amendment remains separately
reviewed engineering work. A public read-only parser was found and exercised on
the stock binary without a model call; it is not a generic transcript walker or
authority to read arbitrary paths. Full mode/platform/permission/lifecycle
qualification remains required. Version strings alone never prove compatibility.

## Independent review and task reconciliation

Claude Opus 5.5 reviewed the exact ADR bytes, complete source extraction and all
five existing issue drafts. Its final scoped verdict: policy and task coverage
correct, ready to post; no concrete transport implementation approval.

- All 15 existing #249 acceptance criteria remain verbatim and ordered; three
  stock-client/source-authority criteria were appended. Why, proposed design,
  validation, delivery and non-goals retain the original outcome.
- Source extraction v3 validated against the immutable commit/blob. No issue,
  work key, parent, Project or release was created or replaced.
- #16, #17, #18, #19 and #249 were read back with the new source binding. Delivered
  #17/#19 and withdrawn #18 remain unchanged. #249 remains Executing / At risk.
- The fresh Project reconciliation preview had zero planned mutations; no empty
  apply was performed. Source freshness read back Current for the reconciled
  branch source. Default `main` remains older; this does not claim a merge.

This clarification neither narrows the original #249 matrix nor unlocks graph,
embedding or guided-recall implementation. Those retain their own roadmap gates.

## Actual R4 campaign: failed, retained

The [source-bound disposition](data/2026-11-06-omp-native/stock-r4-disposition-20261108.json)
records the actual stock 18.16 print run against AM `77cf838`. Native exited 0
with one stored human and one substantive Sol completion; both AM workers exited
1 after creating OPEN metadata, before any observed READY, attempt or outcome.
Their exception cause remains unknown because worker stderr was discarded.
Original streams and the disposable World remain retained, not repaired or
counted as source capture.

Independent Opus and Sol triage separated two probe defects from that runtime
failure: lexical route comparison rejected two real companion-start records,
and print-mode terminal `agent_end` was not idle, so the large-source fixture
correctly refused to send. The large-source case was **not exercised**. Neither
defect explains worker exit 1.

No RPC process, switch, fork or automatic continuation ran. Source capture,
causal EOF, shutdown flush and positive model-forgets qualification remain open.
A separately reviewed model-free diagnostic uses new synthetic stream identities
to distinguish the original companion from worker startup; it does not authorize
closing original streams, changing omp or repeating model prompts.

### Model-free follow-up: observer defect isolated

The [subsequent actual diagnostic](data/2026-11-06-omp-native/companion-parser-disposition-20261108.json)
used two new synthetic stream identities, without a model or native process.
The unchanged hook route without the companion returned READY, verified the
neutral hint and closed successfully. With the frozen companion it returned
READY but exited 1 with OPEN only.

The companion cleared a bytearray backing its regex match before reading the
capture groups. An isolated standard-library reproduction produced empty groups
and `ValueError`; independent Opus source/evidence review traced that exception
through the writer and reader catch paths. This explains the new companion arm's
failure and missing observation records. Its absence of a READY log did not mean
READY was absent from transport.

The original R4 failures are consistent with this defect **[INFERENCE]**, not
retroactively qualified or conclusively attributed. Original and new pending
streams remain retained. The successful baseline covers neutral metadata only,
not source capture, learning, native lifecycle or flush timing.

The [corrected-companion smoke](data/2026-11-06-omp-native/corrected-companion-smoke-20261108.json)
subsequently passed once on a new identity: direct stdout and observer frame
metadata matched; healthy EOF, verified neutral hint and actual CLOSED were all
observed. Independent Opus accepted only this apparatus proof. Original and R2
pending states remained unchanged; no baseline was repeated. Synchronous observer
logging perturbs timing, so this does not qualify native latency, wake or clocks.

## Actual R5b campaign: failed after complete transport observation

The [source-bound R5b disposition](data/2026-11-06-omp-native/stock-r5b-disposition-20261108.json)
retains a separate failure against unchanged AM `77cf838`. The learn worker
received contiguous REQUEST 1–141, FINAL 142 and complete EOF, with a healthy
observer and no partial frames. Only attempts 1–42 have verified durable outcomes.
The worker exited 1 and left OPEN; requests 43–141 remain unaccounted.

Source review localizes the failure to an exception escaping the executor.
`run` retains only an error flag and exits before `Coverage.close`; the original
exception type and stage were not recorded. Later sequence refusals explain
propagation, not the first failure. Contention, changed authority and filesystem
errors remain unproven alternatives. A new bounded, metadata-only external
diagnostic is being designed; no original pending state is repaired or waived.

The instrumented run also had substantial executor backlog: request 42 waited
approximately 106 seconds before its verified response. No root-wake reservation
occurred; the queued seal was cancelled. The old switched stream still has a
refused/unresolvable source identity and unresolved transition. A governed
admission was observed, but the positive model-forgets condition is undetermined.
RPC acceptance checks aborted at the closure gate.

Print source capture/CLOSED and fork large-entry storage/EOF/CLOSED are scoped
positives only. Independent Opus confirmed this failed disposition and identified
a harness mislabel: healthy observation with worker exit 1 was called missing
instrumentation. Separating those classifications MUST retain exit 1 and OPEN
as fatal failures. No native retry or stock omp modification follows from this
evidence, and no full #249, wake, timing or platform pass is claimed.

### Distinct model-free executor diagnostic

The [actual direct-route diagnostic](data/2026-11-06-omp-native/executor-fault-diagnostic-20261108.json)
used two new disposable Worlds and unchanged AM `77cf838`, without a native
client, model, provider or token. The neutral baseline verified two hints and
closed with exit0. A real separately held stream lock caused the first hint's
`SourceTransportBusy` to escape at the expected fixed source locations; the next
sequence obtained no durable slot, and the worker exited1 OPEN. Both workers
were terminal, with healthy output draining.

Independent Opus confirmed only the external first-fault observer and induced
missing-slot propagation. The direct tracer/runpy route is not the shell hook
route, tracing perturbs timing, and no elapsed busy duration was recorded.
This does not identify R5b's original exception, repair its source gaps, qualify
automatic wake or close any #249 acceptance criterion.

## Actual R6c: transport closed; assessment remained pending

The [R6c terminal disposition](data/native249-r6c-terminal-disposition-20261109.json)
binds the actual run to AM `dc66d2a` and the unchanged stock binary. Print-short
exited 0; its AM worker exited 0, closed the stream, and verified all 11 source
attempts. No source gaps, executor faults or unresolved transitions were observed.
Input, source-mode and unrelated installation/repository guards passed.

The campaign nevertheless exited 1: its combined gate rejected two valid
`pending-assessment` observations. RPC never started. Transport terminality and
assessment completion are separate axes: finish-task/retain-pending policy permits
this pending state, but it does not establish an assessment or learning success.
The failed campaign and its original pending sources remain unchanged.

The nonce-bearing wake request and clock handshake occurred. The companion
observed cancellation 949.98 ms after that request, with no wake reservation.
Executor-start and reservation-check timing were not observed; this does not
prove the original R5b exception cause. The same helper observed approximately
56.26 seconds of draining after peer EOF.

A separate [model-free profile](data/native249-controlled-source-profile-20261109.json)
used seven controlled source requests through the
installed AM worker. All seven verified and the worker exited 0; canonical bank
and frozen source files remained unchanged. It recorded 3,902 Git subprocesses,
134 full private-enrollment observations and 773 repository-settings discoveries
over 29.62 instrumented seconds. Overlapping cumulative profile timings MUST NOT
be summed into an exclusive cost attribution. This identifies redundant AM-owned
work to investigate, not a native latency qualification or a measured improvement.

No omp source, configuration or live extension was customized. Ordinary stock
authentication-cache refresh was separately disclosed and authorized for this
run. Raw native transcripts remain private. Full print/RPC, positive
model-forgets, lifecycle and platform acceptance remain open.

### Scoped AM enrollment improvement

The [R12 proof](data/native249-enrollment-progress-scoped-proof-20261109.json)
retains 21 passing affected contracts, two independent Opus source verdicts, and
the same seven-request model-free profile on the changed source. Fresh
ordinary/raw discovery and deferred live guards reduce redundant observations;
complete bounded pending-directory inventory replaces absent-slot probes.
Transient churn consumes the original two-attempt budget rather than immediately
creating a lasting registration fault. Malformed reservations, persistent churn,
authority drift and overflow still refuse before unsafe progress.

The frozen uncommitted source identity is explicit; this is not a retroactive
run against a later commit. Earlier R11 negatives remain retained. All controlled
source outcomes verified, but native callback timing and positive automatic
assessment still require an actual stock-client run.
