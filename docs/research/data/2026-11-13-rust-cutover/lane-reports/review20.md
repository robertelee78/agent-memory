# Independent review: omp 18.20.1 source-refresh amendment

Verdict: **APPROVE-WITH-FIXES** — source-refresh documentation only; not native qualification or acceptance of source-loss exclusions.

## Findings

1. **[P2] Correct the baseline of the RPC wire-change claim.**
   - **Patch anchor:** `docs/research/2026-11-14-omp-18.20.1-binding.md:238`.
   - **Evidence:** Comparing reachable upstream commits `650f3a1` and `b7fe8c7` shows changes beyond optional `select` fields: `packages/coding-agent/src/modes/rpc/rpc-types.ts:125` adds `bash.excludeFromContext`, and `:382-383` adds `ready.supportsBashExcludeFromContext`; the latter is actually emitted by `modes/rpc/rpc-mode.ts:1422-1429`. The lane report's narrower statement explicitly compares 18.19 with 18.20.1, whereas the new binding record presents an 18.16-to-18.20.1 refresh. Consequently, “the only wire change” is not an accurate description of that refresh. The cited protocol-version, prompt, get-state and settle bindings themselves are correct.
   - **Required fix:** State the comparison baseline explicitly and include the additional bash command/ready-capability fields for the 18.16 comparison, or narrow the claim to the particular unchanged frame fields used by the smoke. Do not present the 18.19 delta as the complete 18.16 delta.

## Checked conclusions and qualification boundary

- Inspected the requested queued-guidance bindings, including throw/requeue, abort/false-return loss, synchronous scheduler skip routing, nextTurn insertion/defer paths, session_stop dispatch and cap eight. The 18.16 comparison supports their carry-over; autoParallel, ACP and forced-follow-up differences are described correctly. Pending-message counts cannot establish hidden-guidance delivery.
- Traced `session_clear` through its upstream emitter and generic extension dispatch into AM's handler. Stock retains the transcript/reset boundary but discards queued own continuations. AM captures the stored view, cancels calibration and clears its request. The subsequent `resume`/`foreign-hidden` attribution is appropriate; newly stored genuine human messages still establish human origin. `/fork` abort and both switch/branch adapter routes are covered.
- The strict manual-result recipe supports retaining `omp-18.16-full-skill-v1` as a historical recipe identifier. Unknown shapes, Spotlight details and row-prefixed reads do not gain instruction-context authority. Real-provider/native copy and link evidence remains required.
- The adapter and smoke gates target exactly 18.20.1. The Rust Linux allowlist matches the retained release checksums; production clock, parent-image, pidfd, start-time, namespace-stability, nonce and reservation consumers match the described gate. This does not establish embedded-runtime clock provenance or qualify x86_64, native delivery or elapsed-rate bounds.
- The continuationOf/fork treatment and concurrent-shutdown append window are accurately disclosed compatibility limitations, **not established source-preserving behavior**. Their existing release blockers must remain open: neither an intact retired file nor a successfully flushed FINAL establishes coverage of entries outside that captured view. This review does not approve either limitation as an exclusion from the original source-retention and truthful-pending requirements.
- The amendment does not remove the original acceptance matrix or change the 950 ms callback deadline, 100 ms reservation margin, 1.8 s flush budget, stock 2 s handler cap, guidance bounds or contention bound. Historical evidence and controlled probes remain explicitly insufficient for native acceptance.

Verification: read-only source inspection and Git comparisons against reachable upstream pins. No builds, tests, native runs, Python execution or repository writes were performed. Only this review file was written.
