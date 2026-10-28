# Claude native boundary qualification

Actual client: Claude Code 2.1.306, model claude-fable-5-1. Disposable bound
repository, independent local Git transport and private runtime state; no real
publication or production GitHub-governance claim. Settings use the installed
UserPromptSubmit adapter and production Stop asyncRewake configuration. Native
CLI authentication was used without copying its bytes into these artifacts.

Evidence is under [data](data/2026-10-27-boundary-reliability/). Controllers,
source hashes, actual private completion records and redacted native events are
retained. They do not establish semantic completeness beyond the agent's explicit
source assessments.

| Case | Observed result |
| --- | --- |
| Two ordinary native turns, same session | 20.85s and 10.61s; two explicit source assessments, empty pending inventory, quiet Stop responses, task-only final answers `2 + 2 = 4` and `3 + 3 = 6` |
| Final-source streamed different and identical follow-ups | 35.94s; three callbacks share one native prompt ID, three distinct observation IDs, three explicit completions, pending zero, correct final arithmetic answer without bookkeeping |
| Actual no-tools invocation | 7.50s; substantive explanation preserved with brief pending qualification, retained source pending, one bounded continuation, no fabricated completion |
| Native SIGINT then resume | 36.76s resumed invocation; new source assessed and answer preserved, original unavailable source retained pending with original attribution |

The streamed EOF case does not itself provide a native Stop-response event; its
three completion records and empty backlog establish its observed coverage. The
separate ordinary-turn native events and contract tests verify quiet Stop.

Earlier failed trials are retained separately: transcript-only source matching
refused legitimate queued callbacks; an older example misled the model to submit
legacy fields; shell word-splitting assumptions produced malformed identifiers;
and an early no-tools continuation left the SDK result empty after emitting the
answer. ADR-0020's reviewed native-callback refinement, explicit v2 fields,
actionable identifier validation and final-answer preservation address those
observed failures. They are not recast as passing trials.

The first ordinary-turn proof predates removal of the extra transcript gate;
its source hashes record that fact. The final streamed proof exercises the
reviewed callback-witness implementation. The no-tools and interruption proofs
retain their own exact hashes and qualifications. None proves that an agent will
always follow the skill, that unavailable original sources can be reconstructed,
or that a local no-learning assertion publishes memory.
