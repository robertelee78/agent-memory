# ADR-0022 independent Opus design review

Date: 2026-10-28. Reviewer: `claude -p --model opus`, no tools, low effort,
one turn per review. Final source: `26d4e679af5b237085144a243ee9af23b20bffc8`.

The initial design was revised rather than treating a blocked review as approval.

## Initial verdict

**BLOCK**

The design is sound. It is fail-closed, admin status cannot bypass it, team and App membership are not inferred, there is no credential switch or override, actor and allowlist observations are fingerprinted and reobserved before push, and existing gates are preserved. The pending-work honesty language is correct. Three gaps need fixing before acceptance.

### Necessary changes

1. **Handle a transport rejection after API eligibility.** The ADR says GitHub "still enforces the final ordinary push" but does not say what happens when it rejects. The Git transport credential (SSH key, credential helper or another account) may differ from the API identity. Add:
   > A rejected push or merge to the protected branch after API eligibility MUST remain pending with a cause naming the possible API/transport identity mismatch. It MUST NOT be reported as published, and MUST preserve the candidate and local state.

   If landing writes through an API merge instead of a Git push, say so and name the identity that performs the write.

2. **Define what a complete allowlist observation is.** "All pages" is not enough to judge completeness. Specify:
   - follow `Link rel="next"` until it is absent, within a stated page bound;
   - exceeding the bound, receiving a non-200 page, or getting duplicate or conflicting user IDs counts as incomplete or malformed and stays pending;
   - a 404 or missing restrictions on reobservation is a policy change, not a pass.

3. **Add tests for the new negative cases.** The verification list does not cover several MUST clauses. Add:
   - a team-only or App-only allowlist stays pending even when the actor has write or admin access;
   - a matching ID with a mismatched login, and a matching login with a mismatched ID, both stay pending;
   - a simulated transport rejection after eligibility stays pending, not published, with the bank, HEAD and index intact;
   - an unavailable `/user` response (for example, an App or installation token) stays pending.

Nothing else needs to change.

## Revised verdict

**BLOCK** — one fix needed.

All three prior requests are addressed: transport-rejection diagnosis is in T1 and tested, pagination/duplicate bounds are explicit, and negative tests cover unlisted admin, ID/login mismatch, duplicates, team/App-only grants, unavailable `/user`, and post-eligibility transport rejection.

**Required fix**

The verification list conflicts with T1. It says "malformed/unavailable or paginated allowlist observations remain pending." T1 says complete pagination of up to 4 pages of 100 entries is valid evidence. A test written to this sentence would require valid multi-page allowlists to fail.

Replace that sentence with:

> a matching user found on a later page (within four pages) succeeds; allowlists exceeding four pages, a non-200 page, or a broken `next` chain remain pending; malformed/unavailable allowlist observations remain pending;

No other changes are needed.

## Final verdict

**ACCEPT**

The paginated-observation verification sentence now matches T1. Each T1 failure class has a corresponding test:

- non-200 responses
- exceeded bounds
- broken `next` chains, which fall under malformed or incomplete evidence
- malformed entries
- unavailable observations
- duplicate or conflicting IDs
- ID/login mismatch
- a matching user on a later page within the bound

The other sections are also consistent:

- The final recheck covers removed restrictions.
- Missing or 404 responses are not treated as a pass.
- A transport rejection keeps the change pending with the credential-mismatch diagnostic.
- The truthfulness gates are explicitly left unchanged.

No necessary fixes.
