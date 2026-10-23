# Governed correction and forgetting

Work key: `robertelee78/agent-memory:ADR-0005:forgetting`.
The portable [guide](../../skills/agent-memory/governance/GUIDE.md) teaches
`govern_memory.py preview/apply`. The helper uses the existing lifecycle model,
identity resolver, evidence qualification, comparison and observed-write journal.
It grants no authentication, semantic truth, publication or latest-main freshness.

The public `correction-erasure` suite tests actual correction, supersession,
revocation, known expiry, policy retention, explicit deletion and stronger
reactivation. New corrected versions retain the stable identity and actual new
origin; previous origins, conditions, evidence and useful bodies remain in routed
history. Real revoke-to-reactivate works with the compact inactive representation
and a fresh candidate that omits optional policy fields. Confidence never replaces
trust or actual new evidence.

Exact-body scope narrowing retains still-valid older guidance and excludes it on
main. Its `scope-correction` comparison routes to `governance-required`, avoiding
false contradiction and ordinary deduplication. Different meanings/conditions,
other unresolved claims, procedures pending explicit promotion, or reused evidence
cannot bypass the appropriate review. Another developer's unresolved source is
attributed to the detecting developer with a recommendation to clarify; no message
is sent automatically.

Explicit deletion declares its authorized stable IDs. The helper finds linked
derivatives, owned source observations and their original capture IDs, then
requires that complete scope before writing. Captured observations use
`history/sources/<capture-id>/<source-id>.md`; the common format helper validates
that role against the canonical source ID and its first-version derivation link.
Terminal markers include original capture IDs that never became independent active
records, preventing saved-capture admission after erasure. Source/owner identity
lookup also survives inactive-body compaction, so invalidated learning cannot be
mistaken for novel content from a failed lexical match.

Older flat or unresolved source roles remain unchanged for actual identity
review; no reference-order guess or destructive migration is performed. Retained
independent replacements keep their own evidence/body/origin when an obsolete
supersession pointer is removed during deletion. Every deleted identity's versions
and events are removed, leaving only its safe five-field marker. Reviewed ordinary
context edits remove copied assertions; retained complete copies in the router or
context cause refusal. Custom prose in an otherwise empty topic index must be
preserved before its last route is pruned. Unrelated code, index and HEAD stay intact.

Owned projections contain canonical references and complete bank observations,
not independent bodies. Any canonical mutation invalidates an old projection;
rebuilding filters inactive/deleted references. An older consolidation proposal
cannot replay over terminal deletion. New meaningful evidence can requalify an
inactive identity through a new version and current period; a deleted identity
remains terminal. Arbitrary external stores and Git history are separate retention
boundaries, and semantic paraphrase completeness remains agent judgment.

`tests/contracts/test_governance_recovery.py` adds real process interruptions:
partial correction requires owned-lock recovery and rollback before fresh
comparison; evidence changed at readback prevents commit while rollback preserves
the external edit; an exact completed receipt cleans up without source replay;
foreign-project/default-prefix/unknown-field receipts refuse unchanged; and later
manual edits survive historical replay. Configured nested banks use their actual
prefix and do not create a second default bank. Separate tests erase corrected
versions/events and retain custom index context while removing empty maps.

Input, per-file, batch, router, group and proposal bounds are documented in the
guide. Exhaustion refuses without truncating evidence. Full-bank validation still
scales with bank size; these cases establish correctness, not an unmeasured
large-bank performance or full release-completion claim.

Legacy flat source metadata is included in identity comparison independently of
its compacted body. A candidate UUID in unresolved source references requires
explicit provenance-role review, so even a `related-context` judgment cannot
turn the consumed capture into new active guidance. A regression uses actual
capture/consolidation, faithful flat multi-reference layout, revocation, full
inactive compaction and public re-admission preview; a reviewed role/path move
preserves every source byte and returns the existing inactive-lifecycle decision.
