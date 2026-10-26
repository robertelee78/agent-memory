# Renewed dispute resolutions must keep their replay context

A six-session daily-use diagnostic at runtime
`50e13603be7b8df9ae0912795b7f890266ff3688` reached actual yellow/blue
clarification and consolidation, then failed to prepare memory publication.
Both rounds of independent publication and peer recall had completed. The
publication failure preserved local memory and feature work; it did not finish
this scenario or the required reliability gate.

The private traceback localized refusal to `sharing_frontier.prepare_review`:
the Git/source binding still matched, but the reconstructed manifest and pages
no longer matched the proof. The diagnostic is retained at
`/tmp/agent-memory-v003/daily-use-v1-semantic-smoke-6-128-v7-debug`, including
commands, inputs, source bindings and the paused failure. Its continuation
mechanism was stopped without changing the runtime. It is diagnostic evidence,
not a completed acceptance run.

## Cause and correction

Review records an explicit resolution in the private, repository/worktree-bound
dispute registry. Proof validation excludes the validating judgment's own write
to reconstruct the registry observed before that review. Initially, excluding a
resolution meant replacing it with `null`. That worked for the first resolution.
After correction/consolidation changed its exact evidence, a second resolution
replaced an existing one; reconstructing `null` lost the prior resolution that
had been present in the reviewed manifest. A successful review therefore
invalidated its own proof before publication could use it.

The registry now retains replaced resolutions in an ordered `resolution_history`.
Excluding the current judgment restores its exact predecessor and predecessor
history. Rechecking the same judgment is idempotent. Earlier resolutions supply
replay/audit context only: current resolution qualification still checks actual
source versions, claim endpoints, applicability and stronger evidence. An older
proof cannot roll the registry back after sources or reviewed state change.
The existing checksum, private-state ownership, lock, size bounds and journaled
recovery still apply. Canonical Markdown and its public schema are unchanged.
Older registries without renewal history remain readable; no history is inferred
or reconstructed when it was never retained.

## Regression evidence and remaining work

The public-command regression first detects a yellow/blue dispute, then renews
an attributed clarification as its actual source wording changes. It exercises
three resolutions, repeated current-proof checks, refusal of stale predecessor
proofs, preserved predecessor evidence, registry loss and supported recovery,
and unchanged canonical memory, feature bytes, index and HEAD during checks.
Test-only predecessor `04fa1220f283b350b92d14a531fbe47a00851f73` fails at the
renewed proof check, with the other two known-hold controls passing; its owned
run is retained as `renewal-before-owned.stdout` and `.stderr` under the same
private evaluation root. An initial direct runner invocation was refused for
lacking a disposable test World; that apparatus refusal is retained separately
and is not a product regression result.

The initial fix `bcc8197299349bb00416c0dadb4b3dee3264eaf7` passes the three-method
known-hold suite on macOS. The extended recovery/stale-proof checks, native
Mac/Linux cohort and fresh complete daily-use scenario must qualify the final
candidate separately. This small regression does not stand in for the required
12/1,000 and 24/10,000 multi-session schedules or their independent variations.
