# Seed-kind nonvacuity repair: corrected static approval

Reviewer: **codex-review-59**, independent of **codex-root**. Existing work: `robertelee78/agent-memory:ADR-0009:daily-use-reliability`. Final reviewed candidate **c3e82e963d6fd317b2072ca0af36aa0368e8f99b**, tree **fc918a8f913ad16019553592be6fc5165c69aaa9**. Initial proposal **22aa98e9c0494bbdae9d05df3c28e7fb5c8352b4** and its pre-execution finding remain identified below. Comparison parent: **9ecc726f031dc60311d8128114f90f72069c1221**.

**APPROVED as a bounded static correction at c3e82e9.** No remaining concrete blocker found in this narrow delta. This permits a fresh full semantic preflight and then both full required schedules; it is not evidence that those runs pass, not whole #99 acceptance, and not a product/runtime repair.

## Initial field error and correction

The 22aa proposal correctly changed the kind selectors but used `before[0]['metadata']` in both new positive-control assertions. Actual public retrieval rows use **`record`**, as shown by the unchanged `retrieval.py` output constructor and the retained raw results from the prior audit. Those positives would have raised `KeyError` before exercising the negative transitions. I reported that as a pre-execution apparatus blocker. No workload execution or product failure is inferred.

The two-line c3e82e9 follow-up changes both accesses to **`before[0]['record']`**. That now matches the actual public schema; comparing the complete returned record dict covers exact ID, version, conditions, provenance and other metadata, alongside exact body equality. This report approves c3e82e9, not the uncorrected 22aa source.

## The diagnosed selector gap is closed

The `typed_recall` helper now has a keyword-only `kinds` parameter defaulting to the immutable tuple `('user-feedback',)`, converted into a fresh list in each request. Existing calls therefore serialize the same ordinary request values as before, without a shared mutable default or a widened kind filter. Explicit kinds are used only for the affected semantic-fact seed cases.

Before deleting seed-0, the controller asserts its actual kind and performs positive recall from **both the originating session 0 and the older receiving last session**, with the exact seed ID, `component: src/seed/0`, `kinds=['semantic-fact']`, existing trust threshold and no orientation. It requires one result with exact record metadata and body. The later receiver negative uses the same ID/kind/component after deletion-aware publication and refresh, while retaining old-path absence, tombstone and useful new-learning checks.

Before introducing seed-1's hidden/unsafe replacement, the controller positively recalls the original from the **same session 2 receiver** using `component: src/seed/1` and its actual semantic-fact kind. The final negative uses that same selector after the replacement's retained hold/publication/refresh sequence. The raw replacement preservation, explicit old/new held components, dependent route, no-public-body-disclosure and useful independent learning checks remain. Neither negative can now pass solely because it requests user-feedback for a semantic fact.

No new event names are needed: the existing prune/nonresurrection and withheld-independent-learning events occur only after these functions' new positive/negative assertions. Failure prevents schedule completion. No safety, applicability, source, history, comparison, deadline or workload requirement is weakened.

## Evidence carry-forward and required fresh work

AST/source comparison shows only three controller functions change relative to frozen81: `typed_recall`, `prune_and_older_branch`, and `withheld_replacement_independent_path`. All other function source segments are preserved.

The core-only variations do not enter `post_round_semantics`, so neither changed semantic scenario runs there. Their offline helper does use `typed_recall`, but without a kind override; its serialized default request remains `['user-feedback']`, with every other field unchanged. Existing variation evidence can therefore carry forward **at its actual original source and scope**, subject to its own terminal/raw evidence review. Do not relabel it c3e82e9 execution or infer that it validates the corrected semantic controls.

Product runtime, contract-test files/fixtures and registry bytes/modes are exactly identical to **81fec82c1619d20c12473d23e3fdf6706d52b641**. This isolated evaluation-controller correction does not require automatic repetition of already valid native gates. Their actual results and source must still be audited/bound normally; I do not adopt the parent-reported 143-row/715-execution result or a still-running host in this static review. Final release qualification remains separate.

Fresh **small full semantic preflight**, then complete **12-session/1,000-lesson and 24-session/10,000-lesson semantic schedules**, are required on the corrected frozen apparatus. Two standalone cases or a continued diagnostic do not replace either required integrated run. Compile/diff checks alone are development checks, not those outcomes.

## History and claims

The old required12 export now explicitly distinguishes successful controller completion from **unaccepted required semantic qualification**, points to the two wrong-kind findings and retains its original 974 calls/source bindings. Its valid functional/preservation evidence and limitations are not erased. The RCA also retains the controlled supersession of the older in-progress required24 run as an agent-directed abort, not a product timeout, successful cohort or proved final bank state. I reviewed those descriptions, not the abort's live processes or all cleanup receipts.

The design's added nonvacuity requirement accurately requires matched actual-kind positives before mutation and negatives after it. It does not create new product scope or claim v0.0.3 delivery. #98's accepted evidence, operator precision/recall priority and seven-second target are unaffected.

## Binding and release

`source.json` retains both candidate identities, exact final source/tree and seven inspected file hashes/modes, plus controller-function and native-path equality checks. No product imports, tests, workload/model runs, real runtime/bank/key reads, source edits, tracker or external writes occurred. All own read subprocesses are terminal and collected; no Worlds/background jobs exist. **All reviewer holds are RELEASED. External writer grant: NONE / RELEASED.**
