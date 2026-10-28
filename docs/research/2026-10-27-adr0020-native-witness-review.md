# ADR-0020 native callback evidence refinement

Reviewer: claude -p --model opus --safe-mode --tools empty, actual model claude-opus-5-5. Bounded one-turn design review; no implementation execution implied.

**ACCEPT**, with conditions.

**Why it preserves the trust model**
- The trust root was always the owner-only native `UserPromptSubmit` observation: a random observation UUID, native session/lifecycle IDs, and the direct event SHA256. The Claude transcript-UUID gate never authenticated model judgment or semantic completeness. It was only a cross-projection consistency check, and Codex already runs without it.
- Admission still requires the exact observation ID, session, lifecycle, digest, assessment schema and independent publication verification. No-learning and source-to-publication claims stay explicit agent assertions, so nothing real gets weaker.
- The 2.1.306 evidence shows the gate is wrong, not just strict. Queued follow-ups (`absorbed_mid_turn`) carry no `promptId` and no user row. The gate rejects legitimate, correctly hashed sources and pushes implementers toward inventing UUIDs.
- The alternative, matching queue operations by ordering or occurrence, would tie identity to an undocumented and version-specific transcript format. That is worse than relying on the hook contract.

**Conditions before implementation**
1. **Explicit record versioning.** New Claude records use a v2 format with no `source_uuid` field. Readers must reject any v2 record that has `source_uuid` and any v1 record that lacks it. Never mix the two.
2. **Never key on `prompt_id`.** The evidence shows it repeats across three callbacks. The observation UUID is the only identity, and identical-content observations stay distinct.
3. **Recovery gate.** `source_reference` is only a locator. Recovery must re-derive the exact original text, whether from a user row or a queue `content`, and match the stored digest before acknowledging. If no match is found, or more than one candidate matches, the observation stays pending. It is never auto-completed.
4. **Interrupted or undelivered inputs** stay pending unless the agent actually assessed that exact source. Callback existence alone must not imply completion.
5. **Optional transcript corroboration**, if kept, is diagnostic only and never gates completion.
6. **Fixture tests:** streamed different and identical follow-ups, an interrupted queued input, a digest mismatch during recovery, and unchanged v1 readback.

I did not run the attached script, since you said no tools were needed.

The amendment requires distinct v2 observation IDs, strict v1/v2 fields, exact-source recovery, no automatic completion and unchanged admission/publication gates. Independent reviewer agreed that transcript ordering is not a proven native identity binding. Implementation and live acceptance remain separate.
