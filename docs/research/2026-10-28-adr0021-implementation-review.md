# ADR-0021 independent implementation review

Scope: frozen T1 only. T2 implementation was not frozen and is not reviewed here.
Accepted design source: `ee6aeccf2f87424ba589f27eef1c0a5a0a42ee70`.
Reviewer: independent Codex review agent; no production code edits.

## Initial T1 verdict: BLOCK (superseded by the re-review below)

The shared `trust_paths.inspect_entry` evaluator preserves the existing Unix
ownership, root-sticky, private-ancestor, symlink and writable-hardlink policy.
The descriptor walker applies it root-to-leaf and retains named-binding,
descriptor identity/mode/owner, missing-path and recovery-state checks. Protected
0775 descendants remain permitted without chmod; foreign owners and redirected
entries remain refused. Doctor explicitly labels the local transaction-path and
recovery inspection as not exercising remote freshness. Its `setdefault` change
also avoids replacing an earlier useful diagnostic with a missing-installation
mapping exception. No static blocker was found in those changes.

One operation-specific diagnostic requirement remains unmet. In
`memory_refresh.current`, `semantic.final_sources` executes before the new
explicit final-pin `CurrentFailure`. A real local source mutation at this point
raises a plain `ContractError` inside the existing semantic helper.
`current_result` then classifies it as generic prerequisites rather than a
changed final source:

```json
{
  "sharing_state": "pending-inspection",
  "failure_component": "current.prerequisites",
  "failure_code": "refresh-prerequisite-unverified",
  "reason": "A local bank, identity or snapshot prerequisite could not be verified.",
  "remote_freshness_verified": false,
  "canonical_writes_performed": false
}
```

The reviewer independently reproduced this in a disposable real Git-backed
`RefreshFixture`. A patch to the existing fault rendezvous modified
`activeContext.md` at `refresh-current-before-final-recheck`, immediately before
the actual `semantic.final_sources` call. The public `current_result` returned
the object above. This used `python3 -I -B`, candidate source/test imports and
disposable test state; no installed source, real profile or production bank was
modified. The injected mutation remained the fixture's own source change.

Safety refusal still works, and no canonical write or freshness claim was
introduced. The blocker is ADR-0021 T1's explicit requirement for a safe failing
phase and specific `current-source-changed` code to survive to the operation
response. The present generic prerequisite classification obscures that known
final-observation failure.

Before acceptance, classify generic failures arising from final-source/final-pin
validation with a fixed final-recheck phase and `current-source-changed` code,
without echoing arbitrary exception text. Preserve already structured
`CurrentFailure`, authority and hold diagnostics rather than overwriting their
more specific codes. Add a regression that mutates an actual source at the
rendezvous and asserts the checked public response's phase/code and unchanged
write/freshness flags. No broad passing suite needs repetition for this finding.

## Reviewed working-file SHA-256 values

| File | SHA-256 |
| --- | --- |
| `skills/agent-memory/scripts/agent_memory/trust_paths.py` | `d0e70480f2a05c50f5b2777b7dc2023940ebd893e1eeb34666a0874c90c35d12` |
| `skills/agent-memory/scripts/agent_memory/memory_refresh.py` | `8ce3d2d59e311c7789ff0b47cde7468d634a4cd19eaf1557db8865d4d8ec5c14` |
| `skills/agent-memory/scripts/agent_memory/diagnostics.py` | `fdc44e2048e1888f7b1dc5276a8e3031dac4a72c9780cbdcebe6f7a87cfa62aa` |
| `tests/contracts/test_refresh_path_policy.py` | `c873d170561c8bca1678b5e6243d5a8cabea74d70cdaf41f6b9ff8e88e7c3124` |

The implementer reports 15 scoped contracts passed before this review. That
report is separate from the reviewer's independently reproduced mutation case;
the passing suites were not rerun. Actual Linux qualification and release
acceptance are separate evidence, not inferred from this static review.

Read-only AWA status reported a managed repository, version 0.10.15 and 141
managed issues. No tracker writes or lifecycle advancement were performed by
this reviewer; the coordinating parent retains T1 tracking responsibility.

## T1 corrected-source verdict: ACCEPT

The final-source/final-pin block now preserves structured `CurrentFailure`,
authority and hold exceptions, and converts other expected validation failures
to the fixed `current.final-recheck/current-source-changed` diagnostic without
echoing the original exception. This closes the independently reproduced gap.
The reviewer read the exact narrow fix and independently ran its real-file
mutation regression through both the checked API and actual CLI rendezvous:
one test passed in 9.482 seconds. The regression checks preservation of the
injected source change, feature/index state, false freshness and no canonical
writes. No broad previously passing suite was repeated.

This verdict supersedes the initial T1 block only for these updated files:

| File | Accepted SHA-256 |
| --- | --- |
| `skills/agent-memory/scripts/agent_memory/memory_refresh.py` | `6239647d2c850d373d79eca8a1c4d424ffec884a7adfe0f57196f128e1713909` |
| `tests/contracts/test_refresh_path_policy.py` | `98582e314f156a274ab008070d21de995b9b522b41b69755e767d469c8bab562` |

The `trust_paths.py` and `diagnostics.py` hashes remain those recorded above.
Actual Linux path-policy qualification and operator-visible T2/T3 acceptance
remain separate evidence; this is T1 implementation acceptance, not release
approval.

## T2 initial verdict: BLOCK (superseded by the re-review below)

The combined operation preserves separate refresh and source-assessment states,
keeps the earlier assessment-only receipt meaning, restricts compact requests to
explicit no-learning assessments and retains normal exact source/context/digest
completion gates. Distinct observation batches are bounded to 128 entries and one
session/lifecycle; direct stdin remains limited to 64 KiB. No static source
auto-completion or bypass of admission/publication was found. Native OpenCode
retains its explicit post-answer timing exception rather than inventing a new
source identity contract.

Two quiet-operation issues were reported before final freeze:

1. OpenCode's combined successful `source_ack` returned a shortened but still
   explicit JSON receipt body. ADR-0021 requires no successful receipt body on
   the supported quiet routine route. Returning empty tool content can satisfy
   that requirement without promising suppression of the native tool card.
2. Advisory notice deduplication fingerprints refresh only. A new failed batch
   source assessment can consequently be hidden by an unchanged earlier refresh
   failure when another source in the batch completes successfully.

The second issue was independently reproduced using a real disposable
Git-backed fixture and retained orphan transaction journal, not a mocked refresh
result. First, a failed refresh plus a valid assessment stored the notice. The
next lifecycle in the same session contained two observations: one valid
assessment and one wrong digest. The result correctly retained per-source
verification `[true, false]` and overall false health, but set
`notice_required: false`. Quiet emission returned exit 1 with **empty stderr**,
hiding the newly failed assessment. Sources were not falsely completed; the
blocker concerns truthful reporting of a new failure/state change.

That reproduction used `python3 -I -B`, disposable source/ledger state, and
`turn_check.py` SHA-256
`a4933f9b4540bd84b1880329919c3886cca3b9666a82e275dbb1d4a3283e0cde`.
Before acceptance, deduplication must account for newly changed assessment
failures without including routine successful observation IDs that would repeat
the refresh notice every turn. Quiet combined failure output must preserve an
actionable assessment-failure notice as well as refresh truth. Successful native
tool content and these failure/state-change cases need focused regressions.
The implementer is correcting both findings; final source hashes and verdict
will be appended after re-review. No production code was edited by the reviewer.

An intermediate correction now returns empty native routine success and includes
failed assessment identity/request/cause in the notice fingerprint. Static recheck
finds those changes address the original two issues. Before final acceptance,
the quiet CLI's simultaneous-refresh-and-assessment-failure branch must still
make **both pending stages explicit in its single concise notice**. At present
it selects only assessment text, dropping the failed refresh from quiet output.
Full verbose JSON retains both flags, but that is not the routine quiet output.
The reviewer reported this remaining ADR requirement to the coordinator; final
T2 acceptance is pending the corrected output and focused regression.

## T2 corrected-source verdict: ACCEPT

The corrected advisory fingerprint uses stable refresh failure causes and failed
assessment identity, exact request hash and cause. It excludes routine successful
source IDs and irrelevant observation metadata. Consequently an unchanged refresh
failure does not hide a new failed assessment; an exact repeated failure can
still be reported once. The advisory cache cannot create a completion receipt.

The new shared `failure_notice` emits both failed verification stages and both
actionable repairs in one line when necessary. CLI quiet emission and the native
ledger use the same formatter. Overall health remains false/nonzero when either
stage fails; valid source assessment can complete independently of a failed
refresh. Native default success now returns empty content, while the explicit
`diagnostic: true` option retains stage details and original IDs. Native tool-card
visibility is not promised.

The reviewer independently repeated the actual orphan-journal/new-lifecycle
partial-assessment reproduction on the corrected source. It passed: the newly
failed observation is visible alongside the unchanged failed refresh in one
notice, exact repetition is deduplicated, the invalid source stays pending, and
the original journal remains intact. The reviewer also inspected the corrected
focused tests and independently exercised the native plugin quiet-success and
original-session recovery regression: one test passed in 18.244 seconds.
No broad previously passing suite was repeated.

No remaining static implementation blocker was found in the combined route,
exact-source gates, bounded same-context batches, legacy receipt compatibility,
routine guidance or explicit native queued timing exception. Actual multi-turn
PM behavior, command/workspace/repetition counts, injected failure behavior,
native UI/SDK attribution, final Linux qualification and corrective delivery
remain T3 evidence; they are not inferred from this T2 verdict.

| File | Accepted SHA-256 |
| --- | --- |
| `skills/agent-memory/scripts/agent_memory/turn_check.py` | `e49335bc6e866bd279d27ab51d96b1207ced22ac86dabe3d5204b8e548944255` |
| `skills/agent-memory/scripts/agent_memory/codex_boundary.py` | `97f8e9988424e9c32c77b9814e0534f3b33da0d4136a0d1fa917272e6c0da6cb` |
| `skills/agent-memory/scripts/agent_memory/claude_boundary.py` | `8e64a6d1564ed1b894359bd34a919d2ce37cbbbbc4fa0df816c290a3f91492a2` |
| `skills/agent-memory/hooks/boundary-prompt.txt` | `c9ea1f10f50f8dded7dadaaec3e77bda18b3f53016e1fe81079881b2bcb50ef6` |
| `skills/agent-memory/hooks/codex_prompt.py` | `44ee3de9bb38589045e84cbe2a61f58d1bb46e25ef0515dd5522807b6a119131` |
| `skills/agent-memory/hooks/claude_prompt.py` | `83125ab80eef04e2ed0e8ade2bfa9b5f219f5339cf4c6d982fcc4358fc5a2d3b` |
| `skills/agent-memory/hooks/opencode-v2-queue.js` | `fc2a5cebf5261725fd70db430fec3d015d9ee2e6790cb5d1d39203729c636155` |
| `skills/agent-memory/hooks/boundary_ledger.py` | `10b25fd2d4ecb13d402956efbe2c7a6850727b897092f16f021de71cd77b0341` |
| `skills/agent-memory/turn/GUIDE.md` | `e2cf5dd2e34ba2987abeb3814bc05b0c771e0c35bb5ef745516ecfd7123a4e2b` |
| `tests/contracts/test_compact_turn_check.py` | `9f39870271ca693356deb203651de25c786328c67ef78b3e15b5bec7f903d0a6` |
| `tests/contracts/test_opencode_native_boundary.py` | `54cfa40de5ca9cf1b5bd31aea1605137065e563fbfbe512e995b6de3a00ed553` |

This verdict also relies on the corrected T1 `current_result` API hash recorded
above. Only this review document was edited by the reviewer.

## T1 documentation selector correction: ACCEPT

The reviewer independently checked the retained replacement map: 115 literal
command prefixes in 37 guides now use the existing supported interpreter launcher
instead of direct `python3 -I`. All other bytes in 36 guides are unchanged from
their base. The turn guide's command-line multiset preserves the exact script
paths and arguments alongside its separately reviewed routine-route edits.

The existing, unchanged `hooks/python3.sh` selects Python 3.11+ and executes the
requested script with `-I -B`. This removes the documented direct-invocation
bytecode-cache footgun without changing operation logic or blessing unexpected
installed caches. Its SHA-256 is
`b2d8226d79c09290ddfb15a6ed3a9bd88e93e45b16191f425e0454f6abda8289`.
The checked replacement-map SHA-256 is
`747903828e2ad0266a948941bbdbfe5919274d244c56fd206752bf0d418801a4`.
At this review, the aggregate SHA-256 of compact sorted-key JSON mapping each of
the 37 listed relative paths to its current file SHA-256 is
`ab7dd951bd894d60f322e232410b3ba70be0ef74f4cb1f8101cfcbae75aeb69d`.
That aggregate includes the turn guide's then-current text. Future guidance edits
need their own hash binding; they do not retroactively change this snapshot.
No installed source, real profile or runtime code was modified by the reviewer.

## Native operator-visible follow-up remains separate

Actual native qualification subsequently exposed malformed first-use assessment
construction, legacy Claude fallback instructions that explicitly repeated an
already emitted answer, and OpenCode control-pass bookkeeping/probe calls after
an intentionally empty acknowledgment. These are failed operator-visible trials,
not successful T3 qualification. The coordinator is correcting the complete
early schema and no-repeat/terminal guidance, and researching supported native
control behavior. Prior static T2 acceptance covers only the hashes explicitly
listed above. Final amended guidance/native-control hashes and actual PM counts
must be re-reviewed; no runtime UI capability or SDK attribution is inferred.

## Amended root-owned guidance: ACCEPT for static correctness

The reviewer read the early complete assessment schema, required nested fields,
authored-candidate qualification, no routine bookkeeping narration, both Claude
fallback no-repeat clauses, guide structure correction and explicit SDK
control-record attribution limit. These changes address the observed instruction
conflicts without inventing completion or a host UI field. Actual source reviews
and considered candidates must still be authored; the illustrative empty array
is not a mandatory empty judgment. Durable wake limits and source coverage gates
are unchanged.

| File | Accepted amended SHA-256 |
| --- | --- |
| `skills/agent-memory/hooks/claude_stop.py` | `b4d309cc4cc4a0350ba0da7ec1aa4ada76659e5eec584baf782f232fd4ede05d` |
| `skills/agent-memory/hooks/claude_prompt.py` | `55d61c708c1054f80aba2352853eec5fda5cf980b06b8a104697a0bf93a1ca99` |
| `skills/agent-memory/hooks/codex_prompt.py` | `4ca08a799965f697b24abca9e6fa895ccee8c970312143a16a4a4903d2e8cdde` |
| `skills/agent-memory/hooks/boundary-prompt.txt` | `9f7a856b1d7259f5dc169b3ad5670de155d11f4939ba2a62e3dac513191108f2` |
| `skills/agent-memory/turn/GUIDE.md` | `4e7e57e4da6251d90dacd80ad0115bba1a66a86328e2563a274dee2130901e64` |

Native PM proof under this final guidance is still in progress. OpenCode's new
terminal-control implementation is not yet frozen and is not accepted by this
guidance-only rebind. Failed prior narration/repetition trials remain failures.

## Retained Codex and Claude native evidence: ACCEPT within tested scope

The reviewer read both native reports, controllers, retained event projections,
derived metrics and tested source maps, without rerunning models. Independent
recomputation from the event projections matches the retained maintenance-call,
assistant-message and helper exit/output counts. Controller inspection shows
fixed bounded native invocations rather than a hidden rerun-until-pass loop.
They seed an existing approved project decision before the baseline, inject the
specified orphan recovery journal, and inspect resulting state; they do not
preauthor or submit source assessments. The assessment JSON in the actual native
tool commands is model-authored. The retained failed trials are separate trials,
not silently removed retries.

**Codex:** four same-session PM turns each use one maintenance call and no request
workspace. All four have one final business response, no commentary business
response, and no duplicate refinement question within the turn. First-use guide
loading is a separate counted call, not routine maintenance. The first two checks
exit zero with zero output bytes. The injected recovery failure produces exit 1
and one actionable 315-byte notice; the next check remains exit 1 but emits zero
bytes for that unchanged failure. Task answers continue. Retained metrics report
all exact sources complete and bank/feature/index/HEAD/branch state preserved;
that source coverage does not claim freshness on either failed-refresh turn.

**Claude:** the first malformed-schema/inventory/retry/repeated-answer trial fails
acceptance and remains failed. The next shape-corrected trial also fails routine
first-use quietness because it narrates successful bookkeeping. Its third turn
nevertheless supplies valid, separately scoped injected-failure evidence: one
maintenance call, an explicit transaction-recovery notice, preserved journal,
one task response, completed assessment and false refresh health.

The final quiet-corrected two-turn trial has one maintenance call per turn, no
workspace, one substantive response per turn, no repeated question and no routine
bookkeeping announcement. Its first turn has two additional explicit first-use
guide/recall calls; the warmed follow-up has no additional tool call. Both
observations complete with an empty pending inventory and unchanged fixture
state. The model's shell wrappers echo `exit=0`; those status echoes are not
successful receipt bodies or invisible tool output. The helper itself is quiet.
Native result rows retain the original task responses in these successful cases.

The Claude quiet proof and earlier injected-failure proof use identical combined
helper, refresh and assessment runtime hashes. Their routine prompt/guide hashes
are different and remain explicitly bound in the per-case source maps. Codex's
tested prompt/guide also predates later reviewed wording. This is a composite
evidence review, not a claim that every trial ran every later changed byte.
First-use recall, native tool cards and SDK/control attribution remain visible
qualifications; neither fixture source assessments nor these counts authenticate
universal semantic correctness or publication.

| Retained artifact | SHA-256 |
| --- | --- |
| `data/2026-10-28-quiet-boundaries/codex/events.json` | `2f5998707f0e2c62df8c5414a6a69def5bf79c6c708ceb857c23f1e4efd6bc9b` |
| `data/2026-10-28-quiet-boundaries/codex/metrics.json` | `15897ee058dd438465f28fe5c0ec0f248a751667c0387f7977c32f1956e7e397` |
| `data/2026-10-28-quiet-boundaries/codex/source-hashes.json` | `9125b2cb3f63daa629d2210948ac5307291b46646d75e379dfb36e11d69438df` |
| `data/2026-10-28-quiet-boundaries/claude/quiet-corrected/events.json` | `0ec75e05962545733a2829cd82f1b1d5bb4611c69ff78314bac4d3131a74b238` |
| `data/2026-10-28-quiet-boundaries/claude/quiet-corrected/metrics.json` | `def6fe4399b7a091851a4d5ddcbec9581a2084f6718104a23e5251ff03608909` |
| `data/2026-10-28-quiet-boundaries/claude/quiet-corrected/source-hashes.json` | `979031e40a77991865e4ef88cfef75218e53d8cc5d78062f07ae4329cbff62d3` |
| `data/2026-10-28-quiet-boundaries/claude/shape-corrected/events.json` | `31bcad2d5d03bf31804576dbdcce4ad6ef82482a2a155c85ee5db2db904ce377` |
| `data/2026-10-28-quiet-boundaries/claude/shape-corrected/metrics.json` | `e4ab6870fa9a8dcfd4f5c462d90f46227a1b2d9442a23b578e24be08c3030359` |
| `data/2026-10-28-quiet-boundaries/claude/shape-corrected/source-hashes.json` | `75ab8f9c3d7ebdd5bfc31e48e01ad91a879761c8ef4b17b11451c81f1fb04f20` |

T1's accepted production code hashes were independently compared with immutable
candidate `0a4a544efab187693bd5fb90c9f5dfd831fd92c8`: `trust_paths.py`,
`memory_refresh.py` and `diagnostics.py` all match the accepted values above.
This binds the T1 static verdict to that candidate; it does not independently
advance AWA phase.

The two clients' retained claims are supported within these limits. Overall T3
and release acceptance remain incomplete pending final OpenCode implementation
and native evidence, the real Linux path-policy case and required corrective
delivery checks. Hidden model/provider stop tokens cannot be inferred merely
from a normal finish or zero visible text; any later OpenCode control review
must distinguish observed configuration, actual output/finish and causal
inference.

## Initial exact-control terminal implementation: BLOCK (superseded below)

The reviewer read ADR-0021's accepted terminal-control amendment at
`c2f4e58ce49736f2685491e377135c811b8e39e1` and the frozen
`opencode-v2-queue.js` source with SHA-256
`439f5bd8848ecc6ec3b5e42ca55fa5f4c48469a26d3163b00a86097c19061cc3`.
Its focused terminal-context test source SHA-256 is
`b81eabaff6bda64138558a0ccfa31558d07eb188ce884f9cf084de57b76a801a`.
The examined guards correctly require actual combined refresh/assessment success,
same native session, exact retained control/source-window hashes, acknowledging
assistant ID, current native and outgoing tails, and the qualified provider/model.
Recovery, pending stages and plugin restart do not reconstruct green transport
state from an assessment-only receipt. No session interrupt is invoked.

However, admission epochs reset after independent bounded-map eviction. An
in-flight old acknowledgment can capture epoch 1; 128 other admitted sessions
evict its epoch entry; a new operator prompt in the original session then sets
the default-zero epoch back to 1. If the native projection has not yet published
that new input, the old combined-green response can re-arm a terminal tuple
whose epoch now incorrectly matches.

The reviewer independently reproduced this through the actual plugin's prompt,
source-acknowledgment and context callbacks with mocked native projection/backend
I/O. The old combined response was deliberately held in flight, 128 other session
prompts were admitted, then a new original-session prompt was admitted before
releasing the old response. With the projection still showing the old control,
the plugin removed tools and set its stop option. The observed result was:

```json
{"new_input_admitted_before_terminal_context":true,"old_terminal_armed_after_cache_eviction":true}
```

This is an invalid-arming guard reproduction, not a real model run or evidence
that actual operator work was interrupted. The requirement is nevertheless
explicit: a newly admitted input must invalidate the old green tuple. A
nonrepeating admission nonce or global generation, retained in the bounded map,
must replace the resetting per-key counter. The eviction/in-flight/new-input
sequence needs an integrated regression before static acceptance.

The coordinator separately reports a failed actual terminal trial: stop/tools
configuration was sent, but visible tool markup followed. Its outgoing-request
observer identified later generic foreground instructions overriding the intended
terminal instruction order. That trial remains failed. A supported callback-order
correction requires integrated wrapper/queue tests and final source re-review;
cross-RPC event-object identity is not assumed. No stop-token match or quiet native
success is inferred from configuration alone.

## Corrected exact-control terminal implementation: ACCEPT for static correctness

The reviewer read the final nonrepeating generation and callback-order correction.
Admission generations now use a process-instance-wide BigInt counter with bounded
per-session storage; eviction returns null rather than resetting a reusable
counter. `canArmTerminal` refuses absent/evicted generations, recovery, diagnostics,
failed stages and cross-session observations. The generation is checked again
after awaiting the native projection before caching a terminal tuple.

The reviewer independently repeated the exact original mocked-native
eviction/in-flight/new-admission reproduction. It now passes: the newly admitted
input prevents stale terminal shaping even when its native row has not yet
appeared. This resolves the reproduced ABA blocker without persisting green
transport state or changing source assessment semantics.

The wrapper registers generic foreground context before the queued adapter's
context. The integrated test now chains all registered callbacks rather than
overwriting a previous callback, and asserts that the exact terminal instruction
is last. There is no cross-RPC event-object identity assumption. Tool removal
and stop shaping remain restricted to the qualified model and the exact
already-acknowledged source/control/assistant/tail hashes; ordinary/new operator
input, missing evidence, pending stages and recovery retain their normal tools.
No session interrupt or automatic assessment is introduced.

| File | Accepted corrected SHA-256 |
| --- | --- |
| `skills/agent-memory/hooks/opencode-v2-queue.js` | `ac986b110c0ec949c8108d1eb4a66e220b1492a0661c0714dc3137ce78613c05` |
| `skills/agent-memory/hooks/opencode-v2.js` | `7dee34e7f1ab5f1fa2dcadb5a8b68fc9c731e53af3d4f20f4522a810b673b5fc` |
| `tests/contracts/test_opencode_terminal_context.py` | `9c897ed980436778ba711646db98c4cf437008ed994fe1a6abbc86ddf0d0d243` |
| `tests/contracts/test_opencode_native_boundary.py` | `718a1f31ff6f679bd91ef68ae899a3e13cfc7b73ff18dbc4d8e06c900f498a53` |

The independently executed full-wrapper quiet-success/ordering/original-session
recovery regression passed: one test in 15.010 seconds. No broad suite or native
model was rerun. This static acceptance permits a bounded native qualification attempt; it does not
claim that the model/provider emits or honors the sentinel, that native control
output is empty, or that SDK final-result attribution preserves the business
answer. Those are separate actual-native observations. Earlier visible-markup
and repeated-null-call trials remain failed evidence.
