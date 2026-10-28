# v0.0.9 native Claude PM dialogue

Actual Claude Code 2.1.306, fable (reported canonical claude-fable-5-1),
production prompt/Stop hooks in disposable bound-repository fixtures. Native
assessments were authored by the model, not by the controller. A previously
approved decision was seeded before the baseline; user prompts confirmed that
decision and requested PM recommendations and follow-up questions.

## Failed trials and corrections

The [first trial](data/2026-10-28-quiet-boundaries/claude/first-shape-failure/metrics.json)
failed first-use acceptance. After reading only the opening of the guide, the
model invented assessment fields, then used inventories and a retry. The existing
Stop fallback explicitly instructed it to repeat the business answer, causing a
duplicate. That trial is not a quiet pass. The complete request shape now appears
at the opening of the guide and required nested fields in the native prompt.
Both fallback repeat instructions were removed; original answer records remain
retained and host SDK final-control-row limits are qualified.

The [shape-corrected trial](data/2026-10-28-quiet-boundaries/claude/shape-corrected/metrics.json)
used one maintenance call on each of three turns and zero workspaces, preserving
the bank, code, index, HEAD and branch. It still narrated successful first-use
memory handling. An explicit native-prompt instruction now excludes announcements
of guide reading, assessment or saved/unsaved memory on routine success.
The third turn's injected orphan transaction caused a truthful pending refresh
notice while its exact source assessment completed. Its native final response
retained the business answer once, with the specific governed-recovery next step.
The original journal was preserved; no green freshness claim was made.

## Targeted quiet qualification

The [final targeted two-turn trial](data/2026-10-28-quiet-boundaries/claude/quiet-corrected/metrics.json)
passed the corrected behavior: one assessment/refresh maintenance call per turn,
zero request workspaces, one substantive assistant response per turn, no repeated
business question or routine memory narration. First-use guide loading and
existing-memory inspection were two separate calls, counted rather than hidden.
The warmed follow-up used only the maintenance call. Both exact observations
completed, no sources remained pending, and all fixture bank/code/index/HEAD/
branch state was unchanged. Durations including model time were 34.38s and14.93s,
with a hard180s per invocation. Native result rows contained the business answer.

[Event projections and exact tested pack hashes](data/2026-10-28-quiet-boundaries/claude/quiet-corrected/)
retain evidence for the actual source tested. The injected-failure case above
uses the same combined-check runtime; later changes only tightened routine
prompt wording. No claim is made that instruction compliance proves semantic
correctness, tool-card invisibility or universal host fallback attribution.
