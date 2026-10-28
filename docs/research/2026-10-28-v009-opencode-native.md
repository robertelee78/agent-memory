# v0.0.9 OpenCode native product-manager qualification

This records real OpenCode 2.1.3 standalone runs using `baseten/moonshotai/Kimi-K3`, not a scripted model response. Each model invocation has a hard 180-second deadline; timeouts are failures. The fixture isolates HOME, configuration, credentials copied into private fixture data, runtime state, and Vox configuration/data. No production service was restarted or installation changed. All Python controllers use isolated mode and disable bytecode.

## Initial frozen-source result: not qualified

Private evidence directory: `/private/tmp/am-v009-opencode-pm-3vkwp7xw`. It retains the exact controller, source hashes, JSONL events, native session export, summary, and private memory bookkeeping. Credentials are not copied into that proof directory.

The fixture seeded an already-approved decision before its baseline: three top-level verdicts, with `unknown` reasons `inconclusive` and `analysis_incomplete`, retaining supporting evidence. The model read the actual record. Its assessments were authored against the native original sources, with actual already-recorded evidence and temporary/open claims classified as non-durable.

| Native invocation | Exit | Seconds | Observed result |
| --- | --- | --- | --- |
| PM decision explanation and evidence-display question | 0 | 84.418 | Original answer/question retained once; one `am.source_ack`, but an unnecessary `execute { return null; }` and a final bookkeeping message followed. Quietness failed. |
| Temporary mockup follow-up and next PM question | 0 | 89.304 | Original answer/question retained once; one `am.source_ack`, followed by the same unnecessary tool call and bookkeeping prose. Quietness failed. |
| Injected authority outage | 0 | 79.839 | Original answer retained; one source acknowledgment returned combined `verified:false`, `refresh_verified:false`, `assessment_verified:true`, and an actionable authority-advertisement failure. One brief pending qualification followed. |

There were zero private request workspaces. All three exact source assessments completed; no source intents remained. Memory files, feature code, index, HEAD, and branch matched the baseline. Successful native acknowledgment content was empty (the native code-execution wrapper rendered it as `null`), rather than a receipt wall. The source-assessment receipt did not turn the failed remote refresh green.

The JSONL stream and exported native session distinguish the original business-answer message from the later queued control messages. The last streamed text on successful invocations was bookkeeping rather than the business answer. The native original answer remained present; an SDK consumer that chooses the final streamed text still sees that control message. These are different properties and neither is presented as the other.

## Root cause and narrow correction

When the native queue is active, the context adapter returns before adding the shared boundary prompt. Its existing foreground system instruction delegated work to the continuation but did not explain intentional empty acknowledgment output or successful termination. The queue told the model to avoid final prose, but the tool description did not establish an empty result as the terminal success signal. Kimi treated the empty result as something to follow with a null-return tool call and reassurance.

The correction adds the same explicit rule to the existing system instruction, the source-acknowledgment tool description, and the queue's terminal instruction: empty successful content is deliberate; end immediately with an empty assistant response, without additional tools, a null-return call, or bookkeeping prose. Structured pending output remains a failure requiring one actionable qualification. This uses existing native fields and APIs; it adds no UI suppression, session cancellation, or new protocol.

The existing actual-plugin regression passed after this change in 15.494 seconds, checking empty default success, structured diagnostic opt-in, exact original-source recovery, bounded queueing, and explicit terminal guidance. That contract test does not prove model compliance.

## Corrected native qualification

A fresh, targeted native PM qualification is recorded under `/private/tmp/am-v009-opencode-pm-corrected-fbkf50_o`. Its controller stops after the first invocation if extra text remains; follow-up and injected-failure runs occur only after the first quietness check passes. The corrected first invocation timed out at 180.006 seconds and failed qualification. It retained the original PM answer once and completed one source-bound assessment with an empty acknowledgment result, then repeatedly invoked `execute { return null; }` seventeen times instead of terminating. No successful control-pass prose appeared, but nontermination is not quiet success. The controller killed the process group at the deadline and skipped follow-up and failure reruns. Memory, feature code, index, HEAD, and branch remained unchanged; zero private request workspaces and one completed source assessment were retained.

This is a regression from the initial source's bounded but noisy completion. The explicit empty-response instruction is insufficient to establish a native termination mechanism for this model/harness combination. The corrected source is not qualified for release on that basis. Reverting only this prompt tightening restores the prior known behavior; achieving genuinely silent native termination requires a documented harness mechanism or an explicit product compatibility decision, not claiming that the model's null-tool loop passed.

## Protocol research and proposed request-local termination

The empty-terminal prompt tightening was reverted; the prior combined acknowledgment and quiet result remain. No session-interrupt workaround was added.

The [official server-plugin reference](https://dev.opencode.ai/v2/docs/build/plugins/server#model-requests) documents request-local context shaping of tools and generation options, including stop sequences. Its tool result and session hooks do not expose a per-control finish result. The installed legacy `@opencode-ai/plugin` package is 1.18.13, so its declarations were not treated as the contract of the running 2.1.3 binary.

Pinned upstream v2.1.3 source was fetched read-only into `/tmp/am-v009-opencode-upstream`:

- [Session execution](https://github.com/anomalyco/opencode/blob/v2.1.3/packages/core/src/session/execution.ts#L149) delegates interruption by session ID, without an expected control/message guard. [The coordinator](https://github.com/anomalyco/opencode/blob/v2.1.3/packages/core/src/session/run-coordinator.ts#L142) interrupts the active execution and clears recorded wakes. That operation is inappropriate for finishing only our acknowledged synthetic control while preserving concurrent user steering.
- [Request assembly](https://github.com/anomalyco/opencode/blob/v2.1.3/packages/core/src/session/model-request.ts#L236) applies context-hook tool filtering and generation options to the outgoing request. [OpenAI Chat serialization](https://github.com/anomalyco/opencode/blob/v2.1.3/packages/ai/src/protocols/openai-chat.ts#L819) forwards generation stop sequences. [Baseten](https://github.com/anomalyco/opencode/blob/v2.1.3/packages/ai/src/providers/baseten.ts#L23) uses that protocol.

Proposed narrow repair, pending ADR amendment and independent review: retain an ephemeral exact-control marker only after actual combined refresh and source assessment both verify. Revalidate the original native source, retained synthetic trigger, acknowledgment assistant record, and absence of newer real user/steering input. Only the next outgoing request for that same acknowledged control may remove tools, request one unique sentinel, and configure that sentinel as its provider stop sequence. This gives the model a representable response whose body is omitted by normal provider stopping, rather than asking it to produce no response.

A failed refresh, denied or unavailable source, recovery/cross-session assessment, changed control, or newer user admission cannot arm or use that path. A legacy `.assessed` receipt alone cannot arm it. The marker makes no semantic judgment and is not a persisted admission/publication receipt. Request-local shaping must never interrupt the session or delete queued input. If steering arrives after the old request snapshot, it must remain available to the following normal turn. The original business-answer row and final control/SDK result must still be reported separately.

No blind native model retry or sentinel implementation was performed before the requested design review.

The v2.1.3 [tool snapshot](https://github.com/anomalyco/opencode/blob/v2.1.3/packages/core/src/tool.ts#L256) includes the Code Mode `execute` wrapper in its request definitions. Request assembly constructs context-hook tools from those definitions and emits only surviving entries. Removing all request tools therefore also removes `execute`; it does not leave an empty namespace inside an available execution wrapper. The Chat protocol preserves historical messages and emits an empty tools list when historical tool calls exist. There is no typed context-hook `toolChoice` override: unrecognized context options become provider options, whereas canonical tool choice comes from a separate internal input. No invented `toolChoice: none` context option is proposed.

The first optimization must be qualified specifically against the observed `baseten/moonshotai/Kimi-K3` Chat path. Other providers, including protocols that require historical tool definitions, are not covered by that qualification. A sentinel is successful only when it is the first visible output and the provider ends normally without extra tools or pre-sentinel prose; an error, unsupported stop setting, missing terminal response, or timeout remains a failure.

## First request-local sentinel qualification: failed, with ordering defect

Private evidence: `/private/tmp/am-v009-opencode-pm-terminal-09q_libg`. The selected PM invocation completed with exit 0 in 86.339 seconds. It completed one actual source assessment, created zero request workspaces, and preserved the bank and feature/index baseline. It did not pass quietness, so the controller skipped follow-up and outage repeats.

The qualification-only observer captured the actual outgoing request with `baseten/moonshotai/Kimi-K3`, no tools, and stop sequence `AM_CONTROL_DONE_4acdbf449fc8f14047ced26f`. The original business answer/question remained once. The final native control assistant row completed with `finish: stop` but contained visible serialized tool-call markup for `execute { return null; }`, so it is failed qualification rather than empty completion.

The same observer revealed a concrete ordering problem: after the queue's exact-control hook added the terminal instruction, the wrapper's later context hook appended its foreground instruction requiring the continuation to perform the governed workflow. That foreground instruction was the actual last system part; the intended terminal instruction was not last. A proposed correction is to mark only the already-validated shaped event in a private WeakSet shared by the two adapter modules. The wrapper can then return for that exact event without appending its ordinary foreground guidance. No source identity would be inferred from a string marker; eligibility still comes from the actual combined check and exact native revalidation. This ordering correction and any further native check require review; the failed run is retained unchanged.

## Ordering and admission-generation correction

Independent review identified a second race in the first sentinel implementation: bounded per-session counters could evict an old session, then assign its reused counter value to a newly admitted prompt while an old acknowledgment remained in flight. The cache must not confuse those generations even if native projection has not caught up.

The corrected source registers the ordinary wrapper context hook before setting up the queue, so the queue's validated terminal shaping runs last. It relies on documented hook order, not shared JavaScript object identity across callbacks. Admission generations now use a monotonically increasing BigInt across sessions within the plugin instance, while the session lookup remains bounded. An evicted or absent generation is `null` and cannot arm termination. Re-admission never reuses an old generation. Source-window and control-record hashes are also rechecked before shaping.

Three focused tests passed in 23.208 seconds: twenty exact-context/arming assertions including the precise eviction/in-flight/new-admission race, a real pending-refresh/recovery no-arm test, and the actual full-wrapper plugin test that chains every registered context callback and verifies terminal-instruction ordering plus new-prompt invalidation. This corrects a former test-stub limitation that replaced callbacks instead of chaining them.

Frozen source hashes for independent static review:

- `hooks/opencode-v2.js`: `7dee34e7f1ab5f1fa2dcadb5a8b68fc9c731e53af3d4f20f4522a810b673b5fc`
- `hooks/opencode-v2-queue.js`: `ac986b110c0ec949c8108d1eb4a66e220b1492a0661c0714dc3137ce78613c05`

No further native retry was started before independent static acceptance of these corrections. The previous failed native results remain failures.

## Final selected-model native result after ordering/race correction: not qualified

Independent static review accepted the corrected source before one further selected native run. Evidence directory: `/private/tmp/am-v009-opencode-pm-ordered-jl0qu3j3`.

The PM invocation completed with exit 0 in 63.284 seconds. The actual outgoing context observer confirmed the terminal instruction was now the last system part, the tool set was empty, and the request stop setting was `AM_CONTROL_DONE_155f9eb025b5f72c48983883`. Despite this, Baseten/Kimi-K3 emitted visible serialized tool-call markup for `execute { return null; }`, rather than the requested terminal sentinel. The exported final native control record finished normally with `stop`, but that is not evidence of sentinel matching and is not quiet success.

The original business answer/question remained once; there was one actual successful source assessment, zero request workspaces, and unchanged bank/feature/index baseline. Follow-up and outage repetitions were skipped because the selected quietness case failed. No further model trials were performed.

The ordering and generation-race corrections passed their focused tests, but this provider/model is **not qualified** for the tool-removal/sentinel optimization. Its demonstrated behavior cannot support enabling that optimization under ADR-0021's qualification requirement. A product compatibility decision or a different documented native termination facility is required; more prompt-only retries are not release evidence. All failed qualification artifacts remain retained.

## Experiment removed; original combined-check baseline restored

The unqualified termination experiment was removed in full: sentinel shaping, admission/terminal caches, arming logic, and experimental hook ordering were reverted. Its dedicated test was removed with it; focused tests of the retained combined-check behavior remain. Commit `4b03db1` retains the experiment in local history, and all failed native proof remains available.

The restored runtime files exactly match the original completed three-turn native qualification's retained source hashes:

| Runtime file | Restored SHA-256 |
| --- | --- |
| `hooks/opencode-v2-queue.js` | `fc2a5cebf5261725fd70db430fec3d015d9ee2e6790cb5d1d39203729c636155` |
| `hooks/opencode-v2.js` | `0a6f44d842940840325fb21d54df1b31e547903cd4d1dc638e6cb0c6cd0ea19a` |
| `hooks/boundary_ledger.py` | `10b25fd2d4ecb13d402956efbe2c7a6850727b897092f16f021de71cd77b0341` |
| `scripts/agent_memory/turn_check.py` | `e49335bc6e866bd279d27ab51d96b1207ced22ac86dabe3d5204b8e548944255` |

This preserves one combined native source-acknowledgment/refresh call, empty successful acknowledgment content with explicit diagnostic opt-in, separate truthful stage health, actionable failure reporting, the bounded refresh subprocess deadline, and shared trusted-path handling. It retains the demonstrated short OpenCode control-pass text limitation; it does not claim full silence. No further model trial or release claim accompanies this restoration. The operator's release-versus-full-silence product decision is separate.


## Final ordered native check and excluded experiment

After the independently reviewed cache-epoch and registration-order corrections,
the selected real native case completed in63.284s with exit0. The outgoing
observer verified that the terminal instruction was last, the tool set was empty
and the unique stop sequence was configured. Kimi nevertheless emitted visible
serialized tool-call markup rather than the sentinel. This is FAILED quiet
qualification, even though the source assessment was truthful and the original
answer, bank, code and index were preserved. No follow-up or failure case was
rerun. No hidden sentinel match is claimed from a normal stop reason.

The complete experimental termination implementation, epoch caches, ordering
change and dedicated experimental tests were removed. The final OpenCode
baseline exactly matches the originally completed three-turn native proof:

| Component | Restored SHA-256 |
| --- | --- |
| queue | fc2a5cebf5261725fd70db430fec3d015d9ee2e6790cb5d1d39203729c636155 |
| wrapper | 0a6f44d842940840325fb21d54df1b31e547903cd4d1dc638e6cb0c6cd0ea19a |
| native ledger | 10b25fd2d4ecb13d402956efbe2c7a6850727b897092f16f021de71cd77b0341 |
| combined check | e49335bc6e866bd279d27ab51d96b1207ced22ac86dabe3d5204b8e548944255 |

The restored full-wrapper regression passed in14.7s. The shipped candidate has
no experimental sentinel transport or whole-session cancellation. It retains the
single exact-source combined check, empty successful helper result, explicit
refresh/assessment stage health, safe state trust and actionable failures.

[Public event fingerprints and counts](data/2026-10-28-quiet-boundaries/opencode/)
retain native IDs and source identities without promoting this other-domain test
dialogue into this repository's project knowledge. Original event bodies remain
in the private local proof directories for review. The functional baseline still
has unwanted control-pass prose and a gratuitous null operation. ADR-0021 now scopes v0.0.9 to the verified functional repairs with this explicit
compatibility limitation, under the operator's reiterated instruction to fully
land the corrective release. Full silence is T4 for v0.0.10. This is not a
retroactive quiet PASS or an invented explicit preference vote. Actual signed
publication and delivery checks remain separate.
