The operator accepted the proposed v0.0.3 recall targets in this active conversation, replying: “that sounds right, but open to product q&a as helpful.” This records that actual operator decision; codex-root is the recorder.

The proposal was sent before optimizing or measuring the candidate. It requires:

- Preserve every currently passing fixed recall case at 1,000, 10,000 and 50,000 distinct lessons, with zero unsafe, stale or incorrectly applicable returns.
- Keep the known raw-literal full-question paraphrase miss visible and demonstrate an actual agent query-reformulation workflow separately; do not substitute a scripted fixture oracle or silently claim direct full-question CLI semantics.
- Reduce median read time by at least 25% versus the published measured baseline for each size. The baseline's two sequential observations are roughly 1.25s / 7.38s / 35.8s; this is a data-specific local criterion, not a cold-cache or statistical guarantee.

Accepted research baseline: https://github.com/robertelee78/agent-memory/blob/94eb04a203e86189ba5bb49c20ce120172f282cb/docs/research/2026-10-25-labeled-recall-baseline.md . The fixed labels/producer were independently approved before measurement at 3aecb2adb425b78019156895fd61b11f0497d661. Independent technical design approval remains https://github.com/robertelee78/agent-memory/issues/98#issuecomment-6401456465 .

This decision sets acceptance targets. It does not accept an implementation, grant a universal recall-quality claim, complete #99's required sharing/recovery schedules, or publish v0.0.3. Remaining qualification and the actual agent-assisted evidence must be independently reviewed before acceptance.
