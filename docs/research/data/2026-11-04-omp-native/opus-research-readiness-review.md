I'll read the three plans, the original review, and the issue contracts.

Several lines were cut off. I'll read the full raw text of the lines that matter.

Line 102 confirms the OpenCode limitation. Next I'm checking that the code each task repairs lives inside the skill directory the plan tells the arms to remove or neutralise.

# Re-review of the corrected #246, #247 and #248 research plans

All three plans are technically ready for research, pending operator approval. Each original finding (B1, N1–N8) has been fixed in the actual plan text, and I found no remaining blocker. I did not edit anything, run anything, change the tracker, or approve anything for the operator.

| Task | Verdict |
|---|---|
| #246 federation catalog brief | **TECHNICALLY READY FOR RESEARCH (pending operator approval)** |
| #247 export-governance brief | **TECHNICALLY READY FOR RESEARCH (pending operator approval)** |
| #248 matched-evaluation protocol | **TECHNICALLY READY FOR RESEARCH (pending operator approval)**: B1 is fixed; one fail-closed item for the protocol to resolve (below) |

## What I read
- **Plans:** I read `/tmp/agent-memory-v15-readiness/{246,247,248}-plan.md` in full, including every line cut off in the normal view: 246 lines 24–31, 247 lines 25–30, and 248 lines 28–40 and 55–65.
- **Baseline:** `/tmp/agent-memory-v15-readiness/opus-research-review/review.md`.
- **Issue contracts:** #246, #247 and #248 in `/tmp/agent-memory-omp-native/milestone-issues.json`, read as data (snapshot updated 2026-11-04T01:19–01:20Z).
- **Source:** the local copy of commit `33c95bbe…` at `/tmp/agent-memory-v15-readiness/206-source/`.

## Findings checked against the actual text

**B1, isolation in a repository that hosts its own memory (248 step 4, "Self-hosting isolation prerequisite"): fixed.**
- **Clean source for both arms:** one cleaned copy of the source and its Git history is built before the arms are paired. It removes or neutralises committed banks, historical bank objects and refs, answer-bearing history, the workspace `skills/` directory and harness discovery directories. The transformation is hashed, and both arms are checked to start from identical code.
- **External runtime:** both arms use a pinned, externally installed, identical release of the runtime, skill and hooks, never the code being edited. The plan also checks that the on arm actually uses it.
- **Off-arm canary:** before any trial, a per-harness canary searches the filesystem, Git refs and objects, HOME/XDG, discovery paths, environment, tool, hook and skill listings, and the network. Inert sentinel files test that the canary can detect what it should. Any finding blocks the pair or marks it invalid. "No recall observed" is explicitly rejected as proof of a clean arm.
- **Scope:** the rule covers any repository that ships an agent-memory-format bank or auto-loadable memory files.

**N1, join-record circularity (246 step 7 = 247 step 8, same text; 246 deliverable contract; 247 output section): fixed.**
- Each brief cites only the join ID and its partner's pre-join revision.
- A separate canonical join record holds both final blob hashes.
- If either brief changes after review, its earlier verdict lapses and the changed bytes plus the updated join must be re-reviewed.

**N2, accepting the artifact vs. the recommendation's disposition (246 step 8, 247 step 9, 248 steps 8–9): fixed.** All three keep the two decisions separate. All three state that a defer or reject of the recommendation is still a valid v0.0.15 delivery once the artifact is accepted, and that no disposition approves #55, #57 or #36.

**N3, stale release label (246 and 247 input sections): fixed.** Both say the ADR's "possible v0.0.4" is historical, v0.0.15 delivers research only, and #55/#57 stay unscheduled. #248's source (ADR-0006) has no such label, so nothing was needed there.

**N4, analysis validity: fixed.**
- **Estimand:** the primary estimand is omp, task-level, with the paired difference averaged over repetitions and repository clustering retained. Codex, Claude Code and OpenCode are secondary, with a Holm adjustment across the three.
- **Few clusters:** the five-cluster bootstrap and McNemar on correlated pairs are explicitly ruled out. The protocol must justify a minimum cluster count or a method that works with few clusters, and fall back to descriptive or inconclusive results if it can't.

**N5, cost realism: fixed.**
- **Arithmetic:** the pilot is 12×4 + 4×2×4 = 80 pairs / 160 runs → 22.4M tokens, $105.60, 1,600 minutes. Confirmatory is 720 pairs / 1,440 runs → 201.6M tokens, $950.40, 14,400 minutes. All figures check out.
- **Caveats in the plan:** the figures are labelled "not realistic or measured". Cumulative input including resent context, plus cache reads and writes, are measured per client and model. API metering is distinguished from subscription or quota pricing. Scenarios run from 0.5× to 10×, including warm and cold cache. Pilot-measured figures replace the placeholders before any budget cap.

**N6, pilot repetitions: fixed.** Four tasks get three repetitions per harness. The plan explicitly says it cannot establish benefit or reliable repository-level variance.

**N7, hidden test oracle: fixed.**
- The plan names the exact four removed modules and bans any copies or Git-history access to them.
- Any other removed path is listed and hashed, and a smoke test that doesn't reveal the oracle is provided.
- An external grader runs on a separate grading copy and never returns test contents to the solver.
- If a candidate cannot run without exposing its oracle, it is excluded rather than the requirement being relaxed.
- I confirmed all four test methods exist at the pin:
  - `test_project_identity.py:338`
  - `test_tombstones.py:100`
  - `test_scope_policy.py:53`
  - `test_bank_usability.py:59`

**N8, OpenCode confound (248, S7 row): fixed.** The known limitation is captured and analysed separately as a confound, with an explicit statement that the withdrawn silence experiment will not be restored. This matches `docs/research/2026-10-28-v009-opencode-native.md:102` at the pin ("retains the demonstrated short OpenCode control-pass text limitation; it does not claim full silence").

## Items for the protocol to resolve (not blocking)

1. **The task code lives inside the skill directory the plan removes (#248).**
   - **What the source shows:** at the pin, the code each self-hosted task repairs sits under `skills/agent-memory/`. The directory includes a discoverable `skills/agent-memory/SKILL.md` (`name: agent-memory`). The oracle tests copy the whole directory as a portable pack (`test_project_identity.py:540`, `test_tombstones.py:33,153`, `test_scope_policy.py:98`).
   - **The conflict:** "remove … workspace `skills/`" would delete the task code. Read literally, "block … if a … skill … is found" invalidates every pair for all four pinned candidates.
   - **Why it isn't blocking:** this fails closed. Pairs are excluded or marked invalid; nothing leaks.
   - **What the protocol should say:**
     - Inert task source, identical in both arms and registered with no harness, is allowed.
     - Only discovery and registration (the manifest, hook or skill configuration) is neutralised, identically in both arms.
     - The canary tests whether a skill can be discovered or activated, and whether bank content is present, not whether source files exist.
     - The grader re-applies or reverses the recorded transformation so the full test module still sees its original `SKILL.md` and paths.
2. **Review references inside the brief (all three).** Each artifact must contain its review and disposition references, so adding them after review changes the reviewed bytes. The re-review rule would then apply each time. Existing practice keeps reviews as separate files (e.g. `docs/research/2026-10-28-adr0021-opus-review.md`), so this is solvable. The protocol should say either that those references point to separately bound records or that adding them triggers a metadata-only re-review.
3. **Pilot budget wording (#248).** Pilot-measured costs can't exist before the pilot runs. The protocol should say the pilot cap is approved against the upper (10×) scenario with a stop-loss, and that the measured-cost rule applies to the confirmatory cap.
4. **Model per harness (#248).** The plan's unit is "harness/model". The protocol should fix one model per harness for the primary estimand, or define how multiple models are pooled.

## What this verdict doesn't cover
- **Plans, not results:** these are method plans. No research findings, evaluation runs, harness, catalog or export work, or feature work is approved or exists.
- **Not re-verified:** I did not check live GitHub, issue changes after the JSON snapshot, blob hashes, current provider prices, or which directories each harness actually auto-discovers. My `skills/` point rests on the static manifest and the tests above.
- **Later artifact paths:** the plans propose `docs/research/v0.0.15-federation-catalog.md`, `docs/research/v0.0.15-export-governance.md` and `docs/research/v0.0.15-matched-evaluation-protocol.md`. The canonical join record goes "in review evidence"; the plans give no exact path, so it must be fixed when the join is made.
- **Remaining gates:** operator approval to start, independent review of the exact final bytes, operator disposition, and signed delivery of the published v0.0.15.