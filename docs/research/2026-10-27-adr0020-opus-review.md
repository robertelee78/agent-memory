The shell returned no output for any command, including `pwd` and `echo`, so I couldn't check these documents against the code. What follows is a document-level design review of ADR-0020 and its RCA. The code-dependent points are marked as such.

## Verdict

The direction is sound and it follows from the RCA. The ADR is not ready to move to accepted or task-ready yet. A few MUSTs are underspecified enough that implementers will make conflicting choices, and one of them (T2's witness) carries real safety risk.

## Blocking issues

1. **T1: "source set" completion semantics are undefined.**
   - "Completion MUST bind the exact observed source or source set" leaves the main question open. Is completion per observation, or per turn snapshot?
   - Specify that an acknowledgment carries the exact set of observation IDs it assessed.
   - Stop is satisfied only when the current set is a subset of the union of acknowledged sets.
   - Without this, the "late steering racing completion" test has no oracle.

2. **T1: the "safe bounded serialization" has no lock scope.**
   - Name the critical section. Is it per checkout+session+turn namespace or global?
   - State the lock primitive and how it behaves on macOS and Linux (`flock` vs `fcntl`).
   - Define what happens on lock timeout. It should keep evidence and stay pending, consistent with the capacity rule.
   - Otherwise T4's race test can pass by accident.

3. **T1: same-digest redelivery is ambiguous.**
   - "Idempotent" plus "MUST NOT claim it counted identical repeated messages" is right.
   - It also means a user who sends the same text twice as two real steering inputs gets one assessment. Say explicitly that this is accepted.
   - Say how it is surfaced, if it is surfaced at all. Otherwise a reviewer will flag it later as a T1 violation.

4. **T2: the witness conditions need a precise conjunction and a freshness rule.** This is the riskiest relaxation in the ADR.
   - The quick path currently fails closed. T2 makes it pass on a receipt plus a cursor plus an authority observation.
   - Spell out every condition as an all-of list.
   - Require that the receipt is bound to the same cursor ID, so a cursor and a receipt from two different refreshes can't be mixed.
   - Require that the full-bank hash comparison covers deletions: the *set* of paths must be equal, not just the hashes of the files that are present.
   - Define what "fresh" means for the authority observation (fetched during this quick check, or within N seconds).
   - The RCA's caution about `resulting_hashes` not proving creation is reflected well. Keep that wording.

5. **T2: HEAD/index "mutual-consistency" isn't defined.**
   - The RCA names three gates. The ADR should state, for each one, whether it is relaxed, kept, or replaced.
   - Check 1: local boundary rejects untracked canonical files.
   - Check 2: HEAD bank equals local.
   - Check 3: HEAD/index inventory equals local.
   - Reviewers need that table to confirm nothing else got loosened.

## Significant, non-blocking

6. **T3: "Internal generated instructions MUST NOT be classified as a new operator decision".**
   - This interacts with T1, because a Stop continuation arrives as user-role text in the same turn.
   - Without retained exact evidence, T1 would record it as a new observation, and every fallback would create work for itself.
   - State the mechanism. For example, Stop records the digest of the continuation it emits, and prompt registration matches against that record.
   - That is the "retained exact generated evidence" the ADR mentions. Make it normative, not implied.

7. **T3: the stdin acknowledgment path needs the same bounds as T1.**
   - Give it a maximum byte size.
   - Require it to reject anything other than a no-learning assessment.

8. **T1 migration.**
   - "v1 evidence MUST remain readable" should say whether v1 records are converted lazily or read through a compatibility shim. Both satisfy the text, but they give different rollback stories.
   - Add a rollback/downgrade note covering what v0.0.7 does when it encounters v2 ledger state.

9. **T4 is long but has no explicit exit criteria for the real Codex run.**
   - "At least two normal completion cycles" should state what is observed for each of the five facts (source tracking, assessment, synchronization, publication, recall).
   - It should also state what counts as failure, mirroring the RCA's "a single successful live cycle did not establish reliability".

## Minor

- **Frontmatter.** `amends: [ADR-0004, ADR-0012, ADR-0019]` is fine. Each of those ADRs should get a back-reference when this one is accepted.
- **RCA §3, "Prompt injection supplies…".** In a security-adjacent repo this reads like the attack. Use "Prompt-time context injection" or "UserPromptSubmit guidance".
- **RCA source link.** `learn.chatgpt.com/docs/hooks` doesn't match the usual Codex docs domain. Verify the URL before publishing, since these docs are public.
- **T2 phrasing.** "MAY recognize" pairs oddly with acceptance tests that require it. If the quick path must succeed after an import, make it MUST under the stated conditions.

## Not verified (needs working shell)

I couldn't check any of the following against the code:

- That `codex_prompt.py` keys by session+turn.
- The three gates in `memory_refresh.py` / `sharing_boundary.py`.
- That Stop rejects a null `last_assistant_message`.
- Whether any locking already exists.

These claims come from the RCA as written. If the shell works again, or you can share those files, I can check each RCA claim against the code and turn items 1–6 into concrete proposed ADR text.
