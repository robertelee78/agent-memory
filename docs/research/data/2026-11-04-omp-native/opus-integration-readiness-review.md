**Verdict: the integration is technically ready for per-story operator approval of all six designs (#206, #244, #246, #247, #248, #249).** R1, R2 and R3 are closed, and I found no new safety contradiction.

This verdict covers the design only. It is not an operator decision, implementation approval or milestone/release approval. #209 is excluded: it is still Designing and has no approval status, and its baseline work stays measurement only. I didn't recompute the plan's SHA-256 or run anything; this is a read-only check of the current plans against the pinned source in `209-source-clean`.

## R1 — prompts while a #244 hold exists: closed

- **The pinned source still blocks prompts today.** `codex_prompt.py` calls `scope_binding.require(root)` and then `turn_sources.register`. Any identity failure falls into the `except` branch, which returns `decision:block`. `claude_prompt.py:38` registers directly. Other callers on the same path are `codex_stop.py:59,63`, `claude_stop.py:31,45`, `turn_check.py:40` and `codex_boundary.list_pending/backlog`.
- **#244 now says what happens and who builds it.**
  - 244-plan:176–186 adds an internal, non-serializable `scope_binding.private_enrollment_context(repo)` plus `recheck_private_enrollment`. It is created only after the exact persistent `layout-frontier-hold-v1` marker, the private binding/index/ledger/journal, the project, remote, checkout, developer registry and the `layout-frontier-v1` capability all check out.
  - `turn_sources.namespace` gets two entry points: normal `require` when there is no marker, and the private-enrollment path when the verified hold is present.
  - Codex's extra `require` call is removed. Both prompt hooks then return the real prompt, a real observation ID and `recovery-pending` guidance.
  - Step 2 of the implementation order (244-plan:292) now names `turn_sources.py`, the Codex/Claude prompt and stop hooks, `turn_check.py`, `codex_boundary.py` and `claude_boundary.py`, so the earlier gap in the change list is closed. Normal `require` stays as strict as before.
- **The context gives no canonical or acknowledgement authority.** `complete`, `verify_assessment`, canonical reads, refresh, admission, publication and acknowledgement all keep their normal guard and stay pending (244:180). The context can't be passed to `scope_binding.context_value` or proof checks, and can't be loaded from JSON (244:178). An observed source is never treated as a no-learning or publication receipt.
- **Bad holds still refuse.** An unknown, nested, foreign, corrupt or stale hold, or a missing capability, takes the original refusal path, with "no catch-all `continue`" (244:178, 186). A pin that changes late leaves the registration "unverified/pending": the intent and source are kept, the hook still fails, and no fresh observation is issued. An exact retry reconciles the original intent and never takes a second source slot for the same event (244:182–184).
- **#249 uses the same context.** execution-design-v2:116 consumes it in omp admit/ingest/seal and grants "private source retention/registration only". The milestone (line 61) agrees.
- **Tests are added.** 244:335 (the "Harness hold and integrated capability union" row) covers all three harnesses on the same checked context, plus refusal cells, mixed-feature cells, rollback refusal and attestation renewal.

## R2 — catalog tests that need #206's repair: closed

- **Standalone #244** tests only the catalog-off path and reports the warm-hit and forced-invalidation cells as pending integration (244:170, 334).
- **The combined candidate** runs those cells (206:64). #206's repair is not copied into #244 or into #206's build A, and neither story is blocked from starting.
- **Ownership is consistent.** #244 builds the invalidation hook and #206 tests parity, as stated in 244:170 and 334, 206:62 and milestone:61. 206:62 now says #244 implements the API, replacing the earlier wording that #244 only "proposes" it.

## R3 — semantic-sharing position: closed

- 206:46 says #206 does not change the `semantic-sharing-judgment-v2` schema.
- execution-design-v2:116 says this design does not change `semantic-sharing-judgment-v2`.
- #244 still declares the judgment/proof unchanged at 244:162 and 336.

## Other checks

- **New text doesn't conflict with existing contracts.** I checked the lock order (milestone:47; the enrollment context reads the hold without needing the bank lock), the union of private-state requirements, refreshed quiescence before turning on omp under a hold, and "no cached coverage label". None conflicts with the new text.
- **#246, #247 and #248 are unchanged.** Their files are about 9 hours older than the previous review.

## Not blockers — settle at implementation start

1. **Where the intent file lives isn't stated.** 244:182 defines the `hold-source-enrollment-v1` schema but not its location. Pin it inside #244's per-bank private state area and add it to the shared lifecycle check (`required_private_state_capabilities()`). Also state how an `unverified` intent is reconciled after the hold is retired, when the retry goes through normal `require`.
2. **Three places assign the Codex/Claude hold tests.** Milestone:57 gives regression coverage to the integration owner, 244:335 gives the migration to #244, and execution-design-v2:116 tests all three harnesses. These overlap but don't contradict; the acceptance record for each run should name one executor.
3. **One mixed-feature test needs a specific setup.** omp sources enrolled during a hold can't be acknowledged until the hold retires. So the case "omp's marker clears while the layout hold is pending" needs omp sources that were completed before the hold, or none enrolled during it.
4. **`codex_stop.py:59` calls `require` directly** before `namespace`. It falls under the stop-hook migration named in step 2; make sure that call site is actually converted.

## Readiness for per-story approval

| Story | Integration status |
|---|---|
| #206, #244, #249 | Ready (R1–R3 closed) |
| #246, #247, #248 | Ready (unchanged) |
| #209 | Excluded; Designing, no approval status |