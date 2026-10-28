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

## Terminal experiment removed: rollback verified; full-silence acceptance BLOCKED

The final ordered native experiment failed in 63.284 seconds. The coordinator's
retained report records actual empty tools, configured stop sequence and terminal
instruction last, followed by visible serialized null-tool markup and a normal
native stop. This is not quiet success or proof of matching the hidden sentinel.
Follow-up/outage repeats were skipped after that selected case failed. No further
native model trial was run for the rollback.

The unqualified experiment has now been removed. The reviewer independently
verified all four restored runtime hashes against the original completed
three-turn native proof's retained source map at
`/private/tmp/am-v009-opencode-pm-3vkwp7xw/source-hashes.json`:

| Active restored runtime | Verified SHA-256 |
| --- | --- |
| `hooks/opencode-v2-queue.js` | `fc2a5cebf5261725fd70db430fec3d015d9ee2e6790cb5d1d39203729c636155` |
| `hooks/opencode-v2.js` | `0a6f44d842940840325fb21d54df1b31e547903cd4d1dc638e6cb0c6cd0ea19a` |
| `hooks/boundary_ledger.py` | `10b25fd2d4ecb13d402956efbe2c7a6850727b897092f16f021de71cd77b0341` |
| `scripts/agent_memory/turn_check.py` | `e49335bc6e866bd279d27ab51d96b1207ced22ac86dabe3d5204b8e548944255` |

That original source-map SHA-256 is
`9125b2cb3f63daa629d2210948ac5307291b46646d75e379dfb36e11d69438df`.
The reviewer checked that terminal shaping, sentinel instructions, epoch/cache
arming symbols and event-identity workarounds are absent from the active adapter
files. The dedicated terminal-context test and its contract-registry entry are
also absent. The retained full-wrapper test continues to chain all callbacks;
its current SHA-256 is
`79f0e251c779b9e8a4d902751b6ca2de2a5336894e97da2f3187a347c908ef92`.
The coordinator reports its rollback regression passed in 14.7 seconds; the
reviewer did not repeat that already passing run.

**Rollback/source restoration: ACCEPT. No terminal experiment is enabled.** The
restored baseline retains the previously accepted single combined check, empty
successful helper content, explicit diagnostic opt-in, truthful independent stage
health, actionable/deduplicated failure reporting and bounded backend execution.
Its demonstrated native model behavior still includes an extra null-return call
and successful control bookkeeping prose. Earlier static acceptance of an
experimental implementation does not enable or qualify it after this rollback.
Experiment source remains in local history at
`4b03db1422471c206b905834b3cddde07402c5c6`; all failed trials remain failed.

**Full OpenCode silence and complete ADR-0021 T3/release acceptance remain
BLOCKED under the current criteria.** A concrete baseline can be reviewed while
the operator chooses whether to defer that limitation explicitly or hold the
release. Shipping core fixes with an accepted compatibility limitation requires
that product decision and corresponding source-bound scope/acceptance record;
this rollback review does not make that decision or claim full silence. No runtime
files, real profiles or models were changed or executed by the reviewer.

## Baseline release preflight before the scope amendment

At clean baseline commit `81523c9ba447ab78213aa4bf19cd52d75377c26d`, the
reviewer again verified the restored OpenCode/shared runtime and accepted T1
hashes above. The package validator was independently invoked through the
existing supported interpreter launcher (`-I -B`): exit zero, valid true,
version 0.0.9. No native models or public install checks were repeated.

The working source tree already contained ignored bytecode-cache entries before
that read-only validation; the before/after inventory was identical. Those
existing entries were preserved, not removed or blessed as authenticated source.
The release builder uses tracked Git archive bytes rather than ignored caches or
development memory and disables its own bytecode writes. This source-tree check
does not replace signed-installed-source drift verification or the required
actual public delivery canaries.

The coordinator is preparing an explicit narrow v0.0.9 scope amendment under the
operator's existing fix-and-publish mandate. No operator preference vote for a
noise exception is inferred. Final T1–T3 acceptance must bind the exact reviewed
amendment, its independent design verdict and immutable candidate; this preflight
does not itself waive the previous full-silence criterion.

## Final reviewed v0.0.9 scope and immutable candidate verdict

**T1 implementation: ACCEPT** at
`0a4a544efab187693bd5fb90c9f5dfd831fd92c8`.
**T2 implementation: ACCEPT within the amended v0.0.9 scope** at
`7d48d4103e288983afd5e650f66e21e51687230f`.
**T3 prepublication technical qualification: ACCEPT; signed public delivery
verification remains pending.** This is not final delivery acceptance or an AWA
lifecycle update.

The reviewer read the exact amended ADR and retained final Opus review. The ADR
SHA-256 is
`3c385a321f0ca5d56301092798274f281dd416a8814bf6fb3f49a34aba90c88c`;
the Opus review artifact SHA-256 is
`4f98a04beedb2562c4b52d163e3dde1f24ef3e5a450f059a1b2762dff2a87145`.
The final scope review says ACCEPT for the narrow corrective-delivery exception.
Earlier design reviews and failed trials remain separately labeled.

The amendment resolves the prior release-scope hold explicitly. It permits only
the observed OpenCode successful-control prose and single extra null operation,
counts both separately, and defers full native silence to T4/v0.0.10. It does not
claim an operator preference vote. It treats delivering the verified core fixes
as a bounded implementation default under the existing fix-and-publish mandate.
Data integrity, exact source assessment, permissions, original business-answer
preservation, truthful stage reporting and delivery checks remain required.
The removed terminal experiment is not an enabled or qualified feature. This
verdict supersedes the previous full-silence release hold only within that
formally reviewed v0.0.9 scope; it does not recast a failed quietness trial as a
quiet pass or complete T4.

The reviewer independently rechecked that T1's three production files are byte
identical between the accepted T1 candidate and the final candidate. The four
restored OpenCode/shared files still match the completed baseline native proof.
There is no package/runtime/script/test change from the independently validated
baseline `81523c9` to `7d48d41`; the final scope change is outside executable
package behavior. Final accepted critical runtime hashes are:

| Runtime file | SHA-256 |
| --- | --- |
| `scripts/agent_memory/trust_paths.py` | `d0e70480f2a05c50f5b2777b7dc2023940ebd893e1eeb34666a0874c90c35d12` |
| `scripts/agent_memory/memory_refresh.py` | `6239647d2c850d373d79eca8a1c4d424ffec884a7adfe0f57196f128e1713909` |
| `scripts/agent_memory/diagnostics.py` | `fdc44e2048e1888f7b1dc5276a8e3031dac4a72c9780cbdcebe6f7a87cfa62aa` |
| `scripts/agent_memory/turn_check.py` | `e49335bc6e866bd279d27ab51d96b1207ced22ac86dabe3d5204b8e548944255` |
| `hooks/boundary_ledger.py` | `10b25fd2d4ecb13d402956efbe2c7a6850727b897092f16f021de71cd77b0341` |
| `hooks/opencode-v2-queue.js` | `fc2a5cebf5261725fd70db430fec3d015d9ee2e6790cb5d1d39203729c636155` |
| `hooks/opencode-v2.js` | `0a6f44d842940840325fb21d54df1b31e547903cd4d1dc638e6cb0c6cd0ea19a` |

### Prepublication execution evidence

The earlier independently checked Codex/Claude native counts and qualifications
stand. The reviewer additionally recomputed OpenCode's retained baseline event
counts: one source acknowledgment on each of three native invocations, extra null
operations on the two successful controls, and none on the outage control. The
native original business answers remain once; the extra success text is control
bookkeeping, not quiet success. The SDK/final-stream attribution limitation is
explicit in the native report.

The actual outage stage response has combined verified false, refresh verified
false, assessment verified true and a concrete authority-advertisement failure
with an actionable repair. The reviewer independently located the exact same
object in the original private native tool-result stream, matching the public
sanitized extraction at
`data/2026-10-28-quiet-boundaries/opencode/baseline/refresh-stage-result.json`
(SHA-256 `e1564636b1363064350b2d09cbe4f56b9a823c5a5c97f643d23fe25d9bdefd90`).
This verifies the failed-refresh execution gate without inferring freshness from
the completed source assessment.

The reviewer read the real Linux controller and retained results. Ten path-policy
contracts passed in 8.053 seconds, including protected/unprotected paths and
specific final-source diagnostics; four compact-check contracts passed in 10.404
seconds. There were no skips, failures, expected failures or unexpected successes.
The controller records execution on the actual Linux host, plus the actual
operator-bank quick check reporting bank-synchronized, fresh authority observation
and no canonical/index writes. The separate original candidate probe records
bank/index preservation and unchanged HEAD before/after. Native PM client proofs
were run on macOS; this is shared-contract and real-path Linux qualification, not
a claim that native Linux model conversations were executed.

Private source-bound Linux evidence read by this reviewer:

| Artifact | SHA-256 |
| --- | --- |
| `/tmp/am-v009-linux-final.json` | `52f87f4ab75a515bf12ebc07f904c61012ff84376907998d30eb99e41e8d7008` |
| `/tmp/am-v009-linux-final.py` | `4015b7b467efbb700b04d8d89da0ea72145ccb74d6fe675f0aadc6717d9fd8c0` |
| `/tmp/am-v009-linux-candidate-current.json` | `064eda5870c119705187eba45fc7c554cfa316ac36708ae962c2aa71a50e4530` |

No model or broad test suite was rerun for this final scope review. Package
validation and cache-drift qualifications above remain exact: source-tree ignored
caches were preserved and not authenticated, while signed installed-source
verification must still reject unexpected executable drift. Actual signed public
v0.0.9 install/update/no-op, publisher verification, consumer-memory isolation and
post-verification cache checks remain mandatory after publication. Prepared
signed v0.0.8 upgrade worlds are not completed delivery receipts. Final release
source must retain the accepted runtime bytes and receive its own delivery
binding/review; no public delivery claim is made here.

## Final T3 public delivery verdict: ACCEPT for signed v0.0.9

Exact published source:
`07956881057fe29bb3160deaecc23b3e7abc9f93`.
Portable artifact digest:
`992510030a2e163e95d567954c95d6236d2c3ee2e9c22d6e60c3dacfbddbe78b`.

The reviewer independently read public GitHub release metadata, the tag reference
and downloaded manifest for
[v0.0.9](https://github.com/robertelee78/agent-memory/releases/tag/v0.0.9).
The non-draft release and tag resolve to that exact source. Manifest archive
SHA-256 `964e6053ba1615c6473c7f8f87c327abfde0dd0887da652d44d8b8c9f5be66d0`
matches GitHub's archive asset digest; its source and portable digest match the
publisher receipt and all four installed-version diagnostics. No runtime, script
or test difference exists between accepted candidate `7d48d41` and the published
source. Later evidence commits do not change this immutable release binding.

All four actual public canary results pass. The reviewer read every command
record, result and installed JSON version diagnostic in
`/tmp/am-v009-public-delivery-proof/`, verified their collected result hashes,
and checked all 32 command exits are zero. Each platform exercised a public
default fresh install and actual existing signed-v0.0.8 CLI update, followed by
human version/status, authenticated JSON version, update check, public no-op
update, uninstall preview and actual uninstall. Longest completed command was
6.701 seconds; the existing runner bounds each command to 180 seconds. There is
no retry counted as a passing transition.

| Canary | Total seconds | Result SHA-256 |
| --- | ---: | --- |
| macOS fresh | 15.872 | `707e6abbd6330f3661f14eb5c2976e57ae9cdb6b72cdfe0e7cde7194485fddb0` |
| macOS signed-v0.0.8 upgrade | 18.100 | `48945479db73367335a17dde55f03d51e731366528f64f2f9a8428245f484e91` |
| Linux fresh | 18.501 | `14038163777d50689c0864ff5ba17a51792d6726d4024bb85c7b3ddca86fe925` |
| Linux signed-v0.0.8 upgrade | 21.001 | `cbd9d08a84a27a8dcbe1c09e78a04d1d1879c691a3d9e7cb0d58c434aff7ddb5` |

The existing canary runner is unchanged at SHA-256
`3ae630243176d524844d59aafb4e2c03cde70667c45cde1bc12e3a27c45d0d54`.
Its assertions preserve consumer memory, staged code, HEAD/index and original
client text, exclude development memory and `.git`, verify no-op updates and
remove disposable worlds only after successful uninstall/readback. The reviewer
previously read those assertions and rechecked the exact runner hash. Saved
macOS legacy receipt/version and actual update stdout independently confirm
signed 0.0.8 at C92 transitioning to this 0.0.9 source; Linux preparation evidence
and its successful actual CLI transition remain separately retained.

All four installed diagnostics report `ok: true`, no failures/warnings,
publisher authentication verified, signature verification performed, exact
revision/digest and `cleanliness: verified_release`. Their executing package
inventory/route/compatibility check passes. These are actual installed-inventory
and publisher-verification receipts, not merely package-build validation. This
review did not rerun the signature verifier, native models or public transitions.
The bytecode-safe controllers and CLI introduce no verification exemption. The
evidence covers verified inventory before teardown; it does not claim scanning
already removed source directories after uninstall.

Critical proof bindings:

| Artifact | SHA-256 |
| --- | --- |
| `/tmp/am-v009-publish.json` | `d0dc8def6c7ebad197620bce95fb85c591413a1b53d2f2d66463df4000a09feb` |
| `/tmp/am-v009-public-delivery-proof/summary.json` | `e7f1f0b11377fce425fc69e1cffe3ccc162b6d46bf0e8624abebf8e888084f2e` |
| macOS fresh installed-version diagnostic | `e47bf33866c3410aafb59e9ff2309c1674e5cc157feffaf7b804cf7ae8994217` |
| macOS upgrade installed-version diagnostic | `29c68eeaed363e88c2df54f89e95a0638b9404ed1f40efa501fc5a01059ec4e1` |
| Linux fresh installed-version diagnostic | `100be1e50d0521f867cc59e96289ccdd151e21136c62670b42e068029044da8e` |
| Linux upgrade installed-version diagnostic | `578910e6d92314b2f123862126dea17ccd234b84e4a771fb1da8a093e82c9503` |

This closes the previously pending public-delivery review for T3 under the
accepted v0.0.9 compatibility scope. Full native OpenCode silence remains T4 for
v0.0.10; failed experiments remain failed and disabled. Nothing here claims
universal model compliance, semantic truth, new durable admission/publication or
completion of old pending sources.

The operator's separate historical macOS v0.0.8 installed profile currently has
reported bytecode-cache drift undergoing preservation/quarantine review. The
four disposable public canaries neither used nor repaired that profile. Their
clean v0.0.9 results must not be presented as proof that this historical local
incident is resolved, or that actual operator adoption has completed. That
maintenance/adoption proof remains a separate activity; it is not a failure of
these delivered artifact canaries. No runtime or profile files were edited by
this reviewer. AWA release closure and any memory publication remain coordinator
actions rather than effects of this review document.

The durable copies under
`data/2026-10-28-quiet-boundaries/public-delivery/` were independently compared
with the private proofs: all thirteen command/result/diagnostic/summary files
are byte-identical. `publication.json` has identical parsed values with different
formatting; its durable byte SHA-256 is
`5f8595a28e22b105da4a3bf96a682dcd648889dd9867aba998e2a2b3574913b8`.
The other private-path hash bindings above apply to their retained repository
copies as well. No further delivery gate remains from this independent T3 review.
