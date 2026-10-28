# Live Codex boundary qualification, 2026-10-27

Actual `codex-cli 0.165.0` ran through its native stdio app-server protocol.
Native thread-start responses reported `gpt-6.1-sol` with provider `openai`.
Tests used disposable homes, copied candidate packages, a bound disposable bank
and real local bare Git authority. Fixture hooks were explicitly trusted using
the native `hooks/list` current hashes and disposable `config/batchWrite`;
no hook-trust bypass flag or production configuration edit was used.
Each controller had a 450-second hard deadline and all runs terminated normally.

[Normal operation evidence](data/2026-10-27-boundary-reliability/codex-normal/summary.json):
three actual native `turn/steer` calls submitted two identical inputs and one
distinct input under the first active turn ID. Together with the initial input,
all four observations received separate real-model no-learning assessments and
verified completions. A second ordinary turn completed its one observation.
Both Stop hooks returned no entries and requested no fallback. Operator-facing
answers were `2+2=4; this is a fixture result.` and `3+3=6.` No assessment or
receipt was authored by the controller.

[Denied-tools evidence](data/2026-10-27-boundary-reliability/codex-denied/summary.json):
a trusted thread-scoped native PreToolUse hook actually blocked the requested
harmless arithmetic command. One bounded Stop fallback was observed, followed
by a pending qualification without another block. The original observation
remained pending and unacknowledged. The final answer preserved the requested
result: `11. Memory assessment remains pending because tools were blocked.`
This separate run closes the initial normal controller's narrower no-tools
case, where the denial policy was configured but the model abstained from tools.

[Interruption/recovery evidence](data/2026-10-27-boundary-reliability/codex-interruption/summary.json):
native `turn/interrupt` interrupted the original turn after its source had been
registered, leaving one pending observation. A distinct new native session read
the privately retained exact original test input and verified its digest against
the retained observation before assessing it. The model separately completed
that original observation and its current request. Both inventories ended
complete with no pending sources; the cancelled wait was not executed. The
substantive result `8+8=16` was preserved.

Each evidence directory includes the actual controller, package source hashes,
structured outcome, redacted native lifecycle events and extracted native model
metadata. Authentication bytes, client configuration files, raw account/rate-limit
notifications and raw private event streams are excluded. The interruption
snapshot matched all then-current package bytes; earlier snapshots differ only
in subsequently amended native Claude/OpenCode files and the turn guide.
Codex adapter and common source-ledger bytes matched across the three runs.
Native clients may display hook/tool UI; these tests establish quiet task prose
and completion behavior, not invisible UI or universal semantic correctness.

These are no-learning fixtures. They establish no publication, durable-memory
admission, semantic novelty, or production GitHub governance result. The local
fixture authority and synthetic task prompts are explicit qualifications.
