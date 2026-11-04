**Verdict: REQUEST CHANGES.** This is a technical design verdict only. It is not operator approval, implementation acceptance or native qualification.

Reviewed artifact: `/tmp/agent-memory-omp-native/execution-design-v2.md`, all 123 lines, read in full. I could not compute its SHA-256 with read-only tools.

The revision fixes most of the original problems:
- **B2 is closed.** Mapping is now a qualification rather than a precondition, and acknowledgement goes against the digest-verified snapshot.
- **B3 is closed in principle.** The pinned source keeps `event.message` obfuscated for storage and hands listeners a separate deobfuscated copy (`agent-session.ts` 4621–4634). Recording only metadata at `message_end` and taking content from SessionManager avoids that copy.
- **B6 is substantively closed.**
- **Registration idempotency fits the existing code.** `source_reference` is stored in each shared source record and is visible through `inventory()`, so a lookup under a transport lock before `register()` can work.

Three contracts would still produce silent loss or lifecycles that never finish, or leave the implementer without a defined behaviour.

## Blocking findings (highest priority first)

**R1. A lifecycle with a gap has no defined end state, and ordinary native flows produce gaps (B1/B2 not fully closed).**
- **The design contradicts itself.** `segment.json` carries a `gaps` list, yet "unresolved substantive gaps prevent successful seal". It never says whether a segment with gaps is registered or which acknowledgement outcomes it allows.
- **If it is not registered, the gap disappears from the shared ledger.** The early admission source can be acknowledged on its own. Then `inventory()` reports complete, because it only looks at shared sources (`turn_sources.py` 213–237), and `cleanup()` removes the pending-index slot. The gap is left only in `omp-source-v1`, outside the shared backlog. The design says this is "not a second completion ledger", but `--list-pending` would need to scan it as one.
- **If it is registered but cannot be acknowledged,** it holds one of the 4,096 shared `MAX_TURNS` slots (which Codex and Claude also use) forever. It also keeps the B5 marker or omp references alive forever, so rollback stays blocked.
- **Ordinary flows trigger these gaps:**
  - **Early sealing.** "If stop is skipped, sweep/seal on agent_end" doesn't exclude `agent_end` with `willContinue:true`. That case covers stop-time todo reminders, rewind and plan-mode enforcement, compaction auto-continue and async waits (`agent-session.ts` 5207–5238). Sealing there makes the rest of the task "late-substantive-source".
  - **Tool-binding capacity.** The cursor's `tool_lifecycles` capacity is unnumbered and, when hit, "refuse[s] automatic sealing". That brings back B1's long-task problem.
  - **Rewind.** The model's rewind tool cuts the active branch (around 12180), which can produce `cursor-discontinuity`.
- **Fix:**
  1. Seal on `agent_end` only when `willContinue` is not true.
  2. Release a tool binding once its result is captured, and give numeric bounds for the cursor, segment and capture count.
  3. Always register an eligible sealed segment, with its gaps listed, so the shared backlog stays authoritative.
  4. State exactly which outcomes `omp-boundary` accepts for a segment with gaps (for example, published allowed, no-learning refused).
  5. Name the end state for unresolved gaps, including their effect on `MAX_TURNS` and on clearing the marker.

**R2. Classifying control by `customType` alone silently drops substantive work (B4).**
- **Every stop continuation looks the same.** The pinned source gives every `session_stop` continuation the same entry: another extension's `continue`, another extension's `decision:"block"` reason, and the native blocked prompt all produce `customType:"session-stop-continuation"`, `attribution:"agent"` (`agent-session.ts` 5733–5747, 5833–5843).
- **So foreign work is miscounted as maintenance.** Under the table, work driven by another extension's "tests failing, continue" hook becomes memory control and is never learned from, and no gap is recorded. That breaks "must not lose substantive results".
- **Fix, foreign continuations.** Recognise only this observer's own continuation, by matching the stored `custom_message` content against the reserved `generated_sha256`. That is the observer's own generated text, not display payload, so the no-retention rule doesn't forbid it. Treat any other stop continuation as context. Substantive work after a seal must become an explicit `late-substantive-source` gap, never control.
- **Rules still missing.** The fix should also state:
  - **After steering.** Which lifecycle owns assistant text that comes after a steering admission, for example "the latest genuine admission on the branch, while tool results follow their tool-call ID".
  - **Stored-only user entries.** Whether a user entry with user attribution that never had a `message_end` opens a lifecycle. This matters for crash recovery and for `/btw`, which appends user and assistant entries directly (13759–13765).
  - **Routine custom messages.** That todo, checkpoint, rule and plan reminders (4600, 12292, 3691) are captured as context rather than becoming "unknown substantive relevance" gaps.

**R3. Nothing defines how the compatibility marker is cleared, which leaves a permanent downgrade block (B5).**
- **No operation exists.** The design says clearing the marker "requires a compatible fully verified inventory…", but "no … compatibility-reset operation is added". No actor or command is defined.
- **So the block is permanent.** Once omp has been enrolled, downgrading to an omp-unaware runtime is impossible for good, even with zero omp references. That is stricter than the original fix ("while omp references exist") and is not disclosed in the opt-in preview or attestation.
- **Fix:** either define the clearing operation (its preconditions, lock order, inventory scope and invoker), or state explicitly that the block is permanent in v0.0.15 and make that part of the opt-in preview and attestation.
- **Also document a coverage limit.** `rollback_compatibility` only checks `rollback_repositories` (`installation.py` 957–962), so banks outside that inventory get no marker protection.

## Nonblocking concerns (pin these during implementation)

- **Deadline semantics.** Say that the one-second deadline means "stop awaiting", or a budget the helper checks before it starts writing shared files. It must never mean killing the helper. `exclusive()` creates the file before writing it, and an empty `pending/N.json` makes `index()` raise for every harness (`turn_sources.py` 120–121). Also measure the one-second fsync budget on Linux, and for a stop that seals several lifecycles.
- **Concurrent handlers.** `message_end` handling is fire-and-forget and handlers run concurrently (4577, 4646). Specify a per-session single-flight queue inside the extension, and what happens when the transport lock or completion lock is busy (retry, not a gap).
- **Wake fingerprint.** `wake` must pass the existing `inventory()['fingerprint']` (a hash over all source slots). Otherwise `reserve_wake` always returns False. The design's "pending source identity/digests" wording suggests a different value.
- **Duplicate instances.** Two instances with different artifact identities (for example, a manual copy alongside the installed one) would each assign a random lifecycle UUID to the same human input. Remove duplicates by native entry at ingest, under the transport lock.
- **Overclaimed obfuscation.** Only assistant content is stored obfuscated. User and tool-result entries (for example, `cat .env` output) are stored in clear text by the host, and the snapshots inherit that. Reword "stored/obfuscated" and disclose that retention matches the host's and is screened only by the shared check.
- **Advisors.** Advisor tool events reach the main session's handlers with `kind:"sub"` (`types.ts` 548–549). Check `kind` on every event, and confirm in acceptance whether advisor entries appear in the main session's branch.
- **Acknowledgement shape.** `--stdin` accepts exactly one object, but one tool call is said to perform two acknowledgements. Say whether that is two chained commands or a defined array form.
- **Explicit `update --presets all` with a saved omp binding.** Say whether this removes the binding, keeps it, or refuses.
- **Native memory.** omp has its own memory/autolearn subsystem (6159–6190). Address coexistence in the docs and in classification.
- **Retention growth.** Disclose that snapshots grow without limit when no purge exists.

## Sources inspected and evidence limits

- **Inspected:**
  - Designs: v2 in full; the original Opus review.
  - `turn_sources.py`: `namespace`, `index`, `register`, `exclusive`, `inventory`, `cleanup`, `complete`, `reserve_wake`, `verify_assessment`.
  - `boundary_assessment.validate`.
  - `codex_boundary.py` outline.
  - `installation.py`: receipt schema, `presets`, plan defaults, `rollback_compatibility`.
  - `agent-session.ts`: obfuscation and persistence ordering, `session_stop` and the continuation entry, the `agent_end` settle paths, direct appends, `/btw`, autolearn.
  - `session-manager.ts`: `ReadonlySessionManager`, `getBranch` and `getEntries`.
  - `types.ts`: agent identity and `sessionManager`.
  - Native evidence: `feasibility-report.md`, `message-persistence-comparison.json`, `verified-invariants.json` (outline), `exported-evidence.json` (continuation entries), identity `report.md`.
- **Not verified:**
  - The extension runner: how several `session_stop` results are merged, and the handler timeout.
  - Native message types for todo reminders and async-result entries.
  - Whether the binary matches the source.
  - Linux, TUI and print modes.
  - The original #249 issue text, beyond the design's own restatement.
- **The fixtures are not proof.** They did not exercise this revised design. Everything above stays a post-implementation acceptance obligation.