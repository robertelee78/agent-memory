# #248 matched evaluation protocol — independent acceptance

Research implementation: `5e10593ab90bff0578dfbc51ae876802fd3367bf`. Protocol Git blob `8e54995744bb06a9c2d5f63e4b0d1cde0c2409b0`; worked manifest blob `665698570bf70b389d5d69095e479d91713dd4d5`. Original frozen candidate175d21fa26bbef41f8ec859b463876934e16db79 remains historical, not the corrected candidate.

## Operator disposition

The operator explicitly accepted the research and deferred execution/spending: [verified decision6717362264](https://github.com/robertelee78/agent-memory/issues/248#issuecomment-6717362264). Primary reasoning model remains openai-codex/gpt-6.1-sol. Other study decisions, task authoring, corpus/licenses, budget, provider terms, context capture and instrumentation remain separately gated. No study, production harness, coding-benefit or graph delivery is inferred.

## Independent review and correction

Actual native omp18.16.0 selected anthropic/claude-opus-5-5; observed assistant model claude-opus-5-5 in both reviews. Reviewers had only read/grep/glob tools, made no edits and ran no tests. Original author opus-248 and correction implementer omp-248-grader-rework are distinct from the independent reviewer.

Initial review session01a1b98b-7399-754f-8dfc-bfeb044f0334 returned REQUEST CHANGES: the proposed grading copy used the clean base, permitting a no-op false pass. All other research criteria passed. [Full retained verdict6717614800](https://github.com/robertelee78/agent-memory/issues/248#issuecomment-6717614800).

The correction requires immutable base plus sha256-verified seed for the grading tree, hidden oracle/registry/bank restored by bound blobs, solver diff relative to the seeded sanitized root, and real-wrapper no-op-fails/reference-repair-passes qualification controls. No evaluation harness or executable task was implemented.

Independent re-review session01a1b999-bac7-7743-8d7a-330b6c0e1f0c returned PASS for the exact corrected protocol/manifest, resolving the only blocker without a new contradiction. It did not re-review unchanged calculations or claim actual study qualification. Full source-bound verdict follows.

**Verdict: PASS.** The one blocker from the earlier review is fixed, and the new text doesn't contradict anything else in the protocol or manifest. This covers research acceptance only. Nothing was implemented, no study was run, no task is qualified, and no benefit or signed release is shown.

## What I reviewed
- **Commit:** the worktree's `.git` points to `fix/248-grading-baseline`, and `/opt/agent-memory/.git/refs/heads/fix/248-grading-baseline` holds `5e10593ab90bff0578dfbc51ae876802fd3367bf`. I read the files on disk. I couldn't check for uncommitted changes against that commit, because I had read-only file tools and no git commands.
- **Files:**
  - Protocol status block, §1–§2, §4.1–§4.5 (lines 93–253) and §17–§18. The file is 825 lines, up from 818.
  - The full `worked-manifest-scope.json`.
  - The earlier verdict, `/tmp/agent-memory-v15-readiness/248-opus-acceptance-request-changes.md`.
- **Not re-reviewed:** the calculations, the runner observations and unaffected sections. You report the calculation and manifest-check script exited 0 (33 blobs matched, 166 suites, registry valid); I didn't run it.
- I made no edits and ran nothing.

## How the blocker was fixed
1. **The grading tree is now the seeded tree.** §4.4 lines 200–203 say the grader keeps the clean base and the sha256-bound seed outside every solver namespace. The grading tree "MUST be a fresh copy of that base with the verified `seed_patch` applied, then the hidden oracle, registry and bank restored by their bound blobs. It MUST NOT grade an unseeded clean base." The manifest's `grader.grading_copy` says the same, and so does the `seed_patch.qualification` field ("apply to the immutable clean base for every grading tree").
2. **The solver diff is measured against the seeded root.** §4.4 lines 204–207 and `grading_copy` both say the diff is taken "relative to the seeded, sanitized solver root" and applied to the separate seeded grading tree, never to the solver tree.
3. **The two required checks run through the real wrapper.** §4.5 lines 242–249 and `grader.wrapper_qualification` require:
   - an empty diff must fail the target test and must not get an exact pass;
   - the reference repair, measured against the seeded root, must pass the whole oracle module and the baseline-green set;
   - both checks use the seeded grading tree with oracle, registry and bank restored;
   - hashes, commands and exit codes are recorded.

   `reference_repair.qualification` matches.
4. **The do-nothing pass is gone.** An exact pass needs the target test to pass on base plus seed. Seed qualification already requires that test to fail there, so an empty diff can't pass. The reference-repair check proves the wrapper isn't failing everything. The old wording ("original tree", "original-base copy") no longer appears anywhere in the protocol. `base.commit` still being `ce8660a` is now correct, because the seed is always applied on top of it.

## Contradiction check: none blocking
- The diff path is consistent. Product files under `skills/agent-memory/**` are kept unchanged in the solver root, and only allowlisted product-code changes are carried over. So restoring the oracle, registry and bank in the grading tree can't conflict with the diff, and solver changes to the registry or oracle can't change grading inputs. Prohibited-path changes still fail.
- Three wording points that don't block:
  - **Seed step in the solver root.** It's implied but never written as a step. §4.4 says "seeded, sanitized solver root" and "unseeded code in history is the answer", and §6.6 says "seeded workspace". But §6.4 lines 342–345 and the manifest's `solver_tree_transform` list removals and the rewrite without the seed. §4.5 checks that the two solver roots match each other, not that each root equals the sanitized base plus seed. This can't cause a false pass (an unseeded solver root would still be graded against the seeded tree), but it is a validity gap. When qualifying, record a hash of the solver root that is bound to the sanitized base plus the verified seed.
  - **"Registry outside declared rewrite".** In the pass rule and `prohibited_changes`, this wording is now redundant. Because the diff is measured against the sanitized root, any registry change in it is already outside the declared rewrite. Harmless.
  - **§17 "pending".** It is still a historical placeholder; the external receipts carry the real status.

## Gates still open before any execution
- Re-pin to the published v0.0.15 commit and recompute every blob. Rerun all of §4.5 there; drift was already seen (omp preset, more than 200 suites, the #244 sibling test).
- Write the seed, reference repair, smoke test, assignment text and bank for each task. Then run both wrapper checks and record the hashes and exit codes.
- Measure the baseline-green set on the grader host.
- Operator decisions U3–U14. You report the operator accepted the research and deferred execution and spending (comment 6717362264); I didn't open it.
- At least 2 external repositories for the pilot and at least 8 in total for the confirmatory study, with licenses verified.
- The #36 harness, sanitizer, grader wrapper and canaries, plus a sealed runtime or broker before any self-hosted task enters the primary study.
- A context-capture route for each harness that each provider's terms allow. Native memory features listed for the chosen Codex, Claude Code and OpenCode versions. omp docs re-checked against `650f3a10`.
- #255 G1 and v0.0.16 G2/G3/P1 for the mechanism studies.
- A Linux host, and pilot measurements to replace every placeholder cost and power figure.

## Exercised research reproducibility

Actual `/opt/homebrew/opt/python@3.15/bin/python3.15 -I -B docs/research/data/2026-11-05-matched-evaluation/protocol_calculations.py` exited0 from the correction worktree. It recomputed the existing arithmetic/power illustration and validated33boundGitblobs,21committedbankfiles,fullbankremovalcoverage and sanitized registryvalid166suites. These are calculation/source checks, not coding-task or study execution. Historical measured outputs remain unchanged rather than being relabeled as new observations.

## Delivery boundary

This supports research-only acceptance and integration. Final integration, signed v0.0.15 publication, artifact verification and AWA delivery remain pending. Nonblocking future-execution observations in the review remain explicit; original study obligations are not narrowed.
