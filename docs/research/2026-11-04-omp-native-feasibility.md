# omp native feasibility for #249

Research/design only. No product integration or release acceptance. Work key: `robertelee78/agent-memory:ADR-0004:omp-first-class-integration`.

## Pins

- Actual macOS arm64 omp18.16.0 binary SHA-256: `d4a10d97488d6054d2b18198bd24f461079952c7e0fb75ba66336ca0df0cb99e`.
- Upstream source comparison: `can1357/oh-my-pi@650f3a108229ee11bf68ba3377f4e8c3391cd424`.
- Agent-memory shared-core baseline: `33c95bbebef61e3ac6e58c5c59e0d411b6dc63e8`.
- Real provider: `openai-codex/gpt-6.1-sol`, thinking low. A disposable extension requested ordinary approved probe-tool calls; no agent-memory tools or actual memory-bank mutations were exercised.

## Actual native observations

| Scenario | Observed outcome | Limit |
|---|---|---|
| Approved continuation | Hidden agent-attributed context invoked the real model; normal approval was granted; tool wrote only its disposable sentinel | Fixture tool, not canonical memory publication |
| Headless approval refusal | Tool requiring approval did not execute; no sentinel; task answer retained; session settled | Not a sandbox/OS enforcement audit |
| Explicit rejection | RPC approval cancellation refused the tool; no execute or alternate path | Not every approval policy |
| Cancellation | Abort reached the executing tool signal; tool aborted before its sentinel; original answer remained | Not a crash during an actual bank/Git write |
| Steering | New native user input during the continuation stayed in the transcript and was acknowledged once; original answer not repeated | One actual real-model ordering scenario |
| Unavailable-authority error | Injected tool error retained original answer and surfaced unavailable status without alternative execution | Injected failure, not a real authority outage |
| Message persistence ordering | message_end fired before the current message appeared in durable SessionManager entries; ordered semantic projections later matched all five user/assistant/tool-result entries | Requires provisional capture; does not prove all media/artifact/source classes |
| Advisory cap | Native runtime accepted exactly eight advisory continuations, then settled; original answer once | Cap is not memory completion |

All eight scenarios exited 0 and reached settled state. Every original substantive answer occurred exactly once. All continuation entries had `customType:session-stop-continuation`, `display:false`, `attribution:agent`. Assertions are in the exported evidence; raw outputs remain in scratch with published hashes.

Native identity probes (separate model-free synthetic fixtures): new_session uses a new session identity; switch/resume and fresh-process resume preserve the original identity and entry IDs; fork creates a new session but copies entry IDs; whole vs entry fork differ in parentSession shape. getEntries/getBranch copies are shallow and share mutable nested records. Synthetic snapshots were modified only inside disposable sessions to establish this limit. No model call was made by the identity probe. Earlier driver-framing and missing-model-discovery failures are retained in its report and raw evidence; they were not relabeled product failures or full passes.

## Design consequences

1. Never use RPC msg-N or the current leaf as the just-ended message's durable ID. Preserve original source privately before relying on a terminal callback; reconcile later against ordered durable native entries. Pending capture must survive cancellation that skips session_stop.
2. Copy nested data before awaiting. Use actual session plus entry provenance; copied entry IDs alone are not globally unique across forks. Do not mutate native transcript objects or infer parentSession's type.
3. Use native session_stop for a bounded advisory continuation, not agent_end as completion control. No `decision:block`; preserve source-bound pending state on refusal/error/cancellation/cap/scheduling ambiguity.
4. Execute canonical memory workflows only through ordinary tool approval. Installing a trusted observer authorizes narrow private source/transport metadata writes, not a canonical-memory/Git or arbitrary subprocess escape hatch.
5. Reuse existing shared turn_sources/boundary_assessment protocols and their honest agent-assertion/source-publication-link qualifications. Do not adopt the old experimental OpenCode ledger as an omp memory engine.

## Evidence and reproducibility

The accompanying `exported-evidence.json` is an explicit projection of real native transcript entries, with stable native IDs, parent links, semantic contents, agent attribution, scenario results and hashes of retained raw stdout/observations. Credential pin records, internal provider payload/context metadata and model thinking are excluded; this export does not claim full-source coverage. The tested semantic projection preserves keys present from role/attribution/content/timestamp/toolCallId/toolName/isError/details, with missing distinct from null. All five message pairs matched that projection; full assistant raw equality did not hold because persistence adds contextSnapshot/errorId.

Real-model driver: `run_probe.py`; observer: `boundary-probe.ts`. Native commands and unprojected stdout/observations are under `/tmp/agent-memory-omp-native/<scenario>/`. The observer later added message_end logging and the cap branch; these additions do not establish immutable historical fixture hashes for earlier runs. Installed binary and raw outputs are separately pinned. Do not describe the current observer file's hash as every prior loaded fixture's hash. Native driver and exported traces are research artifacts, not installed integration or permanent product tests.

Real-provider runs used disposable cwd/session roots and disabled ambient extensions, skills, rules and context loading. They used the normal existing authentication/config root, with explicit always-ask policy, unlike the fully scratch-root model-free startup/identity probes. No credentials were copied into evidence; no driver setting mutation, live memory-bank access or live integration change was performed. This is not proof that every background subsystem made zero config/cache writes.

Model-free identity reproduction and caveats are recorded in `identity-report.md`; its raw scratch roots remain `/tmp/agent-memory-omp-identity`. Reports distinguish native outcomes from fixture/driver failures.

## Not qualified

Actual agent-memory automatic recall/search-before-write/admission/consolidation/refresh/publication; source-to-assessment completeness; external/oversized tool payloads; real offline Git and recovery; concurrent session source enrollment; lifecycle install/update/rollback/uninstall; Linux; interactive TUI or print modes; Codex/Claude/OpenCode regression. All remain required implementation acceptance work, not waived by these native feasibility probes. Source/binary equivalence, arbitrary version ranges, cryptographic source-publication causality and universal compatibility are not claimed.
