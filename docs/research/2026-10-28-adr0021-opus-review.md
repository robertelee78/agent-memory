# ADR-0021 Opus review

Actual reviewer: claude -p --model opus --safe-mode --tools empty, actual model claude-opus-5-5. Two bounded design-only calls; no execution evidence inferred.

## Initial review

**BLOCK** — the direction is sound; the required changes are mostly about precision.

**Security (T1)**
1. Define "protected ancestor barrier" normatively: nearest ancestor that is owned by the current user, mode ≤0700, non-symlink, and observed by descriptor. Group-writable intermediates are accepted only below that barrier, and only when same-owner and not world-writable. State whether group-writable directories with foreign group members are acceptable, or explicitly scope that out.
2. Require a single shared policy implementation (quick refresh calling `trust_paths`) rather than two harmonized copies. Add a parity test asserting identical verdicts across a path matrix.
3. Add a test showing that a barrier rebound or chmodded to 0755 mid-walk is refused (TOCTOU).
4. Add to T1 the RCA's lesson that diagnostic imports MUST use `-B` or `PYTHONDONTWRITEBYTECODE`. Also add that source verification MUST NOT skip executable caches. It is currently only in the RCA.
5. Enumerate the failure codes and state that they contain no path contents beyond what the operator owns. Doctor should either exercise quick refresh or label it "not exercised".

**Scope (T2)**
6. T2 mixes MUST and SHOULD for the single-call route. Make "at most one assessment command/native call" the sole MUST, and treat per-host combination as implementation detail. Today the SHOULDs could be read as weakening the MUST.
7. "Bounded direct assessment route" and "bounded Stop fallback" need numeric bounds: max continuations and max retries per turn. Also state what happens after a failed refresh on later turns, e.g. report once per session or until state changes.
8. The RCA attributes this incident's duplication to commentary-plus-final emission, not Stop. T2 must name that cause and the remedy (instruction or guide change) explicitly. Otherwise the fix targets the wrong mechanism.
9. Add Codex quiet-success parity with the Claude adapter as an explicit item.
10. "Existing assessment-only receipt compatibility MUST NOT be silently redefined": specify the version or schema field that signals any change.

**Achievable UX**
11. State plainly that the target is "≤1 visible memory tool card per routine turn". Zero is not the target, pending the operator's open question. The RCA says that choice is unresolved, so the ADR MUST record it as an open decision or cite the operator's answer. It cannot remain proposed-ready while the question is unanswered.

**Acceptance (T3)**
12. Make the pass criteria numeric:
    - routine memory calls ≤1
    - private workspaces = 0
    - repeated business answers/questions = 0
    - injected refresh failure yields exactly one actionable message and combined health false
13. Specify how counts are gathered (transcript/event log source per host) and that evidence is recorded verbatim, not summarized.
14. Add the Linux path-mode chain from the RCA (0700/0775 mix) as an acceptance fixture on a real host. Add a negative case for an unprotected path.
15. "Each command MUST fit 10–15 minutes" is ambiguous. Use a hard per-command timeout of 15 min and define how a timeout is treated (fail, not skip).
16. For release, list the concrete checks: signature verification, install/update from public artifact, consumer-bank isolation, and no `.pyc` drift after verification.

**Minor**
- Remove "Technical review precedes execution readiness" duplication. Keep a single gating sentence.
- The `date` field says 2026-10-27, while the RCA cites 2026-10-28 findings. Note the amendment date.

With items 1–3, 6–8, 11 and 12 addressed, this would be ACCEPT.

## Revised review

**ACCEPT**

I found no blocking security or behavior contradictions. The revision fixes the prior precision issues:

- **Authority claim:** The operator's request (fixes plus release) is separated from the implementation choice. The one-command route is framed as a reversible default, with no preference vote and no claim of hidden UI.
- **Path policy:** T1 binds quick refresh to the existing private-ancestor policy through one shared evaluator. It keeps descriptor-pinned, no-follow, root-to-leaf observation. It preserves ownership checks below a barrier and refuses ACL authentication and chmod-to-pass. Tolerating 0775 dirs below a 0700 barrier follows from the existing barrier definition rather than adding a rule. The negative tests (unprotected writable paths, foreign owners, symlinks, rebinding, stale proof after chmod) cover the realistic bypasses.
- **Stage truthfulness:** Refresh, assessment, admission and publication stay distinct. Combined health stays false on refresh failure. The combined response gets its own versioned schema, so existing assessment receipts keep their meaning. Doctor must exercise each check or label it not exercised.
- **Bounds:** The ≤1 routine call and 0 workspaces limits apply only to the routine path. Source recovery and genuine capture are explicitly exempt. The forgotten-operation caps (one per fingerprint, two per lifecycle) and once-per-state-change failure reporting don't conflict with each other.
- **Acceptance:** The thresholds can be measured from native event records. The 15-minute hard timeout counts as a failure, not a skip, which matches the operator's limit.

**Non-blocking nits:**

1. **`-B` vs `-I`:** The rationale is correct: `-I` implies `-E`, so `PYTHONDONTWRITEBYTECODE` is ignored. But "MUST use `-B`" could be read as replacing `-I`, which would drop isolation from the script directory and user site-packages. Say "`-I -B`" explicitly.
2. **Context duplication:** The "not a claim of… / no zero-visibility" idea appears twice back-to-back. Merge the two sentences.
3. **Missing spaces:** "at most64KiB", "observed0700/0775" and "hard15-minute" each need a space.
4. **Group membership sentence:** "Group membership grants no Unix traversal through that barrier" is redundant, since the barrier already has no group access. Either drop it or rephrase it as "group-writable descendants are trusted only via the barrier, not via group membership" to make the intent clear.
5. **"Exactly once" behavior:** T2 says to emit the business answer once, after the pre-answer assessment. Consider stating what happens if the assessment fails: the answer should still be emitted once, followed by one failure notice. That makes "without displacing the task answer" testable.

None of these change the security posture or contradict the stated behavior.

The accepted source incorporates the non-blocking isolation/spacing/failure-output clarifications and makes the one-call budget total routine maintenance calls, with first-use loading counted separately. This is technical design acceptance, not proof of implementation, native UX or signed delivery.

## Native OpenCode timing amendment

**ACCEPT**, with one wording fix recommended before commit. I reviewed the text directly and did not run the pasted script.

**Preserves native contracts:** Yes. OpenCode keeps its existing queued post-answer control pass and its `source_ack` precondition: the original completed assistant record plus control-message context. No new pre-final identity is invented. Codex and Claude stay pre-answer. Automatic capture, the stop-fallback caps and the host-limit disclosures are untouched. No MCP is added and hidden UI is not promised.

**Meets the one-answer/no-workspace budget:** Yes. The control pass must be silent on success, must not repeat the business answer, and keeps the original answer record. It creates no workspace. On failure, OpenCode emits one notice after the single answer, which fits "answer once, one brief failure notice."

**Recommended fix:**
- **T2 call budget:** The text says OpenCode "MAY use one native source-acknowledgment call" but never says that call also covers refresh. Since the cap is "≤1 call total," state it explicitly, e.g. "one native call combining source acknowledgment and quick refresh, reporting through the versioned combined-operation schema."

**Non-blocking notes:**
- T3 should require evidence of which record the OpenCode SDK reports as the final result (original answer vs. control pass), not just a qualification.
- The Context section still says "This proposed corrective source," which is stale under `status: accepted`.

The accepted amendment explicitly retains OpenCode's existing queued source contract, combines its acknowledgment and refresh in one native call, and requires SDK final-record evidence. Codex/Claude remain pre-answer. No new source identity, MCP or hidden-tool-card guarantee is introduced.
