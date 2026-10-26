# Required 12-session/1,000-lesson cohort: independent evidence review

Reviewer: **codex-review-59**, independent of implementer **codex-root**. Existing work: `robertelee78/agent-memory:ADR-0009:daily-use-reliability`. Evidence commit **9ecc726f031dc60311d8128114f90f72069c1221**, tree **642da427cefb26a4bfff576adbcbcc8262132fb7**. Executed runtime/controller source: **81fec82c1619d20c12473d23e3fdf6706d52b641**.

**CHANGES REQUIRED for two vacuous negative recall controls before claiming that every required semantic outcome is qualified.** The retained controller did complete successfully, and substantial functional/preservation evidence is valid. This finding is an apparatus qualification defect, **not a demonstrated product failure**. Do not erase the completed run, relabel it a product failure, or treat this bounded prerequisite audit as whole #99/release acceptance.

## Concrete qualification gap

The seeded records `topics/seed/records/0.md` and `topics/seed/records/1.md` are both **`kind: semantic-fact`**, verified independently from actual retained complete refresh-proposal bytes. The controller's `typed_recall` helper at frozen-source line **368** always requests **`kinds: ['user-feedback']`**. Its two seed negatives inherit that filter:

| Actual command | Frozen call site | Intended gate | Actual request/result |
| --- | --- | --- | --- |
| **916** | line **460**, `prune_and_older_branch` | Obsolete seed-0 guidance stays absent after governed deletion and older-branch exchange | Exact seed-0 ID and component context, but `kinds=['user-feedback']`; zero results |
| **973** | line **554**, `withheld_replacement_independent_path` | A hidden/unsafe replacement prevents its older seed-1 claim becoming current guidance | Exact seed-1 ID and component context, but `kinds=['user-feedback']`; zero results |

These requests exclude the actual target kind irrespective of deletion/replacement. Their empty results therefore do not demonstrate the intended gates. In particular, seed-1's original file is deliberately preserved locally, so file preservation cannot substitute for the missing qualified-guidance assertion.

The old seed-0 file's actual absence/tombstone checks and the selected hidden/linked-old-source body/route holds remain meaningful. No erased or withheld guidance was observed being returned. The defect is that this particular evidence could not detect such an incorrect return under the intended selector.

I found no additional wrong-kind vacuity in the other standalone retrieval calls: their ordinary, retry and wall records are actually user-feedback. The condition-specific retry requests and admitted peer records therefore remain meaningful. This conclusion does not claim exhaustive correctness of arbitrary selectors.

## Minimum valid repair and qualification disposition

Keep the ordinary helper default `['user-feedback']` unchanged. Add an explicit kind argument for these two seed cases, using **`['semantic-fact']`** for both the positive and negative requests. Before mutating seed-0, require exact current ID/version/metadata/body recall under `component: src/seed/0`. Before introducing seed-1's hidden replacement, require the corresponding exact positive recall under `component: src/seed/1`. Then repeat those same kind/ID/context selectors after deletion or quarantine and require no current result. This makes failure depend on the actual gate rather than a selector mismatch. Preserve the existing file/tombstone, source/provenance, explicit held-component, useful independent-learning and feature-state checks.

Fresh **complete required semantic schedules** must use the corrected frozen controller; two detached small negatives do not replace their integration with the full 12/1,000 and 24/10,000 schedules. Preserve the current successful-controller run with the disclosed gap and the earlier incomplete/aborted diagnostics separately. Parent owns any interruption/cleanup of superseded work; I performed none.

The two core-only independent variations do not invoke `post_round_semantics` (frozen line **763**), where these seed cases reside. A narrowly default-preserving helper/call-site repair does not change their exercised ordinary recall, two-round or recovery paths. Carry their actual source-qualified evidence forward if that byte/code-path relationship remains exact; do not rerun them automatically solely for this correction. Any additional runtime/controller changes require their own impact assessment. Native runtime gates are not invalidated by this test-only defect.

## What the retained evidence actually establishes

### Source and publication of evidence

The complete source binding matches immutable Git **758 tracked files and 162 portable files**, including modes. Before/after source-check receipts are equal and match those counts and the bound commit. The retained controller is byte-exact to that commit, SHA-256 **f48fe78daf796a8cfcc1527280b62d82d6c2988a7a995cbf0d4c83730cf73a76**. Its explicit positive hold, seed/mode, marker and exception-retention repairs are the reviewed frozen apparatus, not an earlier continued diagnostic.

All original/export hashes match. The public command export differs from the private ledger **only by removal of each `environment` field**. I independently checked the actual private World capability-token values are absent from the committed exports. The **1,699 gzip files** match their compressed hashes/sizes and their decompressed content-addressed names: **117,083,186 compressed / 404,750,040 decompressed bytes** checked. Every referenced stream/input blob exists and hashes correctly; **716 unique input blobs** are referenced. No raw fixture content, capability token or private environment is reproduced in this report.

### Actual executions and fault outcomes

The ledger contains **974 unique public attempts**, with **966 exit 0**, **seven exit 1** and **one exit 86**. There is no recorded timeout or raised execution failure. The longest logged public call is **54.699595878 seconds**, below the unchanged 300-second worker limit. The retained driver receipt is exit 0; cohort elapsed time is **5,319.442660985 seconds**. This is one observed workload, not a generic latency guarantee or causal speed comparison with the earlier 10k timeout.

The seven refusals are actual lost-page, omitted-edge, duplicated-page, stale-authority judgment, frozen old-reader v2, ambiguous-base and rewritten-base cases. Their raw responses do not claim publication/freshness. The exit-86 call is an actual refresh apply with the declared `transaction-after-receipt` injection and empty stream, followed by explicit lock/transaction recovery and retry. Negative acknowledgment input bytes actually contain the duplicated page or omitted edge, rather than merely naming those cases. For **82 valid reviews**, acknowledged page digests correspond to **289 distinct retained actual public page bodies** with matching binding and no duplicate acknowledgments.

Publication records **51 prepared requests**, **29 actual landed results**, **one already-landed readback** and **22 pending-reconciliation responses**. Each round prepares all twelve original pending writers before landing; ascending then reverse order and eleven reconciliations per round are retained. These are real separate session worktrees across three operational clone roots, sharing one canonical project and two developer identities. Four origin harness values are labels in records; no native Claude/Codex/OpenCode/custom-client process or model invocation is claimed.

The outage is evidenced by actual denial records for the effective session-0 target, remaining active after the eleven verified peer landings and before restoration/rejoin. It is not the earlier globally renamed-and-restored remote experiment.

### Useful learning, preservation and authored semantics

Actual admitted proposal/receipt bytes bind **31 records**: 24 ordinary round records, four wall/retry role records and three later independent additions. Their actual body, origin, evidence and conditions agree with the captured fixture candidates. The twenty-four distinct session/round refresh plans contain every expected ordinary record's exact metadata/body and every original seed payload hash; I performed **444 such ordinary source comparisons**, including the explicit replay. There are **48 retained seed inventory bodies**, covering all twelve sessions at initial/round-1/round-2/final, with exact expected ID/hash/mode sets. Only final sessions **0, 2 and 11** omit designated seed-0, as required by their later refresh participation; the controller also requires its exact tombstone witness. The other seed payloads remain exact.

The raw held publication proofs contain both wall-body components, not just a generic conflict flag. Page/source bindings preserve the source variants used by the scripted fixture judgments. The source and retained inputs exercise same-file compatible edits, disjoint retry environments, explicit attributed clarification and provenance-preserving consolidation, framing-child closure, real old-runtime refusal, deletion/older-branch exchange, caller code integration and quarantined replacement with independently useful learning. These are supplied known fixture meanings, not model semantic-quality evidence or a general contradiction detector. The two negative recall qualification defects above remain exceptions to the claimed complete semantic gate.

All actual public stdout/stderr streams were independently checked for the active negative fixture's body/title canaries; none disclosed them. Private raw inputs legitimately retain the hostile fixture source and are a different surface.

The **164 feature observations** (162 numbered-session entries plus two ambiguity-probe checks) retain content-addressed actual code/index bytes plus HEAD/branch. Sessions 1–11 each have one stable state; session 0 has exactly two states, with the later one attributable to the explicitly expected ordinary feature integration and actual 900-valued code. No other memory operation is reported as an allowed feature rewrite. Source/controller assertions also compare bank state around the caller integration, separating those intentional Git operations from memory-only refresh.

## Evidence limits

The original Worlds are gone; I did not reread their live banks or Git objects. Actual planned payloads, source-bound native responses, retained inventories, controller assertions and completion are the available evidence. Direct fixture Git operations are not all separate entries in the public helper ledger. Moreover, the **31 successful refresh-apply private result bodies are not retained as blobs** after their concise public hash/size summaries; peer-recall equality inside those results is therefore supported by the exact executed controller assertions, public verified summaries and complete input plans, not an independent reread of every complete private result. A future controller can retain these private output bodies by their declared hash as an evidence improvement without changing product semantics. This is a disclosed retention limit, not a new observed failure.

This cohort does not establish 24/10k, the two variations, all host/native gates, #98 quality/latency, signing delivery or whole #99 acceptance. The seven expected refusals and one injected interruption are not hidden failures, while the two wrong-kind negatives are not promoted into successful gate tests.

## Artifacts and release

`source.json`, `evidence-manifest.json`, `audit.py` and `audit.stdout` bind this independent audit. Hash/source/accounting checks report zero discrepancies; the separately recorded semantic qualification findings are the two wrong-kind controls. No product imports/tests/models, new fixture runs, real runtime/bank/key reads, tracker or external writes occurred. All own read/audit subprocesses are terminal and collected; no Worlds/background jobs exist. **All reviewer holds are RELEASED. External writer grant: NONE / RELEASED.**
