# v0.0.9 native Codex PM dialogue

Actual Codex 0.165.0 app-server, gpt-6.1-sol, isolated HOME, real trusted
UserPromptSubmit/Stop hooks and model-authored assessments. A previously approved
fixture decision was seeded before the baseline. Four same-session product
questions ran without memory instructions in the user prompts. A retained orphan
transaction was injected before the third turn and preserved through the fourth.

[Retained native event projection](data/2026-10-28-quiet-boundaries/codex/events.json)
and [derived counts](data/2026-10-28-quiet-boundaries/codex/metrics.json) show one
maintenance call per turn, zero request workspaces, one business final per turn
and zero commentary messages or duplicated questions. First-use guide loading
was one separate call. Normal checks returned exit 0 with no receipt body; the
injected failure returned exit 1 and one actionable transaction-recovery notice.
The fourth check remained exit 1 but suppressed the unchanged notice. Task
answers continued, and the bank, staged code, index, HEAD and branch remained
unchanged. All four exact observations completed; that does not claim refresh
succeeded on the two failed turns. Durations were 33.24, 49.30, 17.83 and 18.16
seconds, including model time. Each turn had a hard 180-second bound.

Source hashes identify the tested pack. Subsequent guide/schema wording changes
and removal of Claude fallback duplication are separately reviewed; this trial
is evidence for the source tested, not a claim that unexecuted changes ran here.
Native tool cards remain visible. App-server task results contained each original
business answer exactly once. No claim is made about hidden cards or general
semantic correctness from a mechanical completion receipt.
