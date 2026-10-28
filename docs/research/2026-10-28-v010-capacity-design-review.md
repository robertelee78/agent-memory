# v0.0.10 capacity diagnosis design review

Design source: `bf0bdb3ca13d76440253454c626f276ff2e42a6d`.
Reviewer: actual `claude -p --model opus`, low effort, no tools.
Operator selected this bounded corrective outcome on 2026-10-28.

## Initial BLOCK

# BLOCK

The safety core is sound:

- **Narrow scope.** Separating structural validity from capacity applies only to a successor derived from verified current state and validated review.
- **Validation order.** Full schema, checksum and every entry must validate before excess is classified.
- **Persisted readers unchanged.** An oversized, malformed or incomplete store cannot become trusted.
- **Limits are exact.** "Per record" history with the current resolution counted separately matches the public repro: 4,096 accepted, 4,097 refused.
- **Refusal ordering.** The refusal happens before any journal, registry, recovery-copy or anchor replacement.
- **Prohibitions are explicit.** No limit increase, pruning or archival.
- **Verification is bounded.** Fixtures come from a real review, unchanged-state hashes are required, negative controls keep their original diagnoses, and the per-test budget is stated.

Two gaps leave acceptance criteria ambiguous, so an implementation could pass with incompatible behavior.

## Necessary changes

1. **Retry semantics at capacity.** "Test idempotent same-proof retry at capacity" does not say what outcome is expected. State both cases:
   - Re-applying a proof already recorded in a registry at exactly the limit SHALL be a no-op success. It adds no entry, raises no capacity error and leaves state hashes unchanged.
   - Retrying the refused over-limit write SHALL return the same typed capacity refusal. State SHALL stay byte-identical, and nothing SHALL be marked completed.

   Without this, either a spurious refusal of a valid no-op or a silent "success" on the refused write could satisfy the text.

2. **Deterministic classification when several limits are exceeded.** The reason names "the exhausted dimension" in the singular, but one successor can exceed more than one limit. Specify one of the following:
   - A fixed precedence: record, then history, then byte.
   - Report every exceeded dimension, using that precedence for the primary code.

   Add an acceptance case where two limits are exceeded at once. Also pin the byte measurement to the complete sealed encoding of the proposed successor, the same value the persisted-state guard checks. That way the boundary cannot differ between the write path and the read path.

With these two amendments applied, I would ACCEPT.

## Revised ACCEPT

**ACCEPT**

The prior blockers are resolved:

- **Exact-limit retry:** Reapplying an already-recorded proof at the limit must succeed as a no-op, with byte-identical state and no capacity error.
- **Refused-excess retry:** Retrying must return the same typed code with byte-identical state and nothing marked completed.
- **Precedence:** Classification is fixed as record count, then per-record history, then bytes, and a combined-excess acceptance case checks it.
- **Byte measurement:** Size comes from the same complete sealed encoding the persisted-state guard uses, not an unsealed payload or journal envelope.
- **Negative controls:** Malformed proposed input stays separated from capacity classification, and persisted-state controls keep their original diagnoses.

No further changes are necessary.

## Independent code-aware design review

`/root/adr20_review` read the actual final amendment and independently accepted
it at that exact source. The review confirmed inclusive limits, validation before
capacity classification, unchanged persisted guards, typed diagnostics,
evidence preservation and explicit retry semantics. Implementation and release
delivery require separate acceptance.
