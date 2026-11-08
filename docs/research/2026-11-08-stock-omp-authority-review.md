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
