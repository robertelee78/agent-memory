# v0.0.14 task-specific design review

These are actual independent reviewer-agent verdicts, not author self-approval,
implementation acceptance, executed contract tests or delivery evidence. The
operator selected #245 and #243 for v0.0.14 and authorized landing the milestone
and publishing after functional work lands. Scope and acceptance remain those of
their existing issues and ADR amendments.

## #245: DESIGN ACCEPT

Reviewer: `EligibilityDesignReview014`, independent of the design/implementation
owner. Reviewed immutable revision:
`c43f80f06cecc49898d6eb5122452f5f18879d27`.
Design: `docs/design/v0.0.14-bank-source-eligibility.md`.
Work key: `robertelee78/agent-memory:ADR-0018:bank-validation-consistency`.
Baseline: `ab1d86e6a7b6f18a41ce09317bc3b8947ec3c368`.

Actual verdict: **DESIGN ACCEPT**, no material blockers; full #245 acceptance is
represented without weakening safety. The reviewer crosschecked the ADR/RCA and
current observation, validation, writer, transaction, discovery/catalog and public
failure paths. It specifically accepted the file-only common observer cutover,
complete-file/aggregate/count gates, exact owned transaction markers larger than
4 KiB, intended single-file preview/apply validation, stable descriptor/name
measurements, safe typed diagnostics and preservation/pending boundaries.

Nonblocking implementation obligation: audit earlier generic guards such as
`sharing_limits.bank_size` and intermediate reclassification sites, not just CLI
exception handlers. Verify the full public macOS/Linux matrix at the frozen
implementation; this design verdict is not that proof. Reported confidence 0.91.

## #243: initial DESIGN BLOCK

Reviewer: `HookDesignReview014`, independent of the task-design author.
Reviewed immutable revision: `c43f80f06cecc49898d6eb5122452f5f18879d27`.
Design: `docs/design/v0.0.14-owned-hook-health.md`.
Work key: `robertelee78/agent-memory:ADR-0007:owned-hook-health`.

Actual verdict: **DESIGN BLOCK**, P1, confidence 0.99. The unconditional proposed
owned-hook dispatch for every `hook-config` would reject valid post-uninstall
transaction outputs: `installation.install` constructs targets with planned
post-removal identities but omits active `hook_entry`/`stop_entry` ownership,
then invokes `verify_targets`. Those outputs may be absent or contain only
unrelated configuration. The design must distinguish exact planned-output
readback from active receipt ownership, without restoring a historical-hash
fallback for insufficient active receipts. Normal uninstall and recovery must
retain executable regression coverage.

The reviewer otherwise found the complete-group/JSON-type-sensitive matching,
Stop/legacy/mode/path/race controls, lifecycle caller inventory, diagnostic scope,
public baseline reproduction and macOS/Linux preservation matrix represented.
A nonblocking resource clarification notes that `MAX_CONFIG + 1` bounds JSON
reading, while existing `fingerprint` separately permits reads through its
64-MiB managed-file guard. No whole-evaluator 1-MiB IO claim is justified.

The blocked verdict remains retained. #243 requires an actual independent
re-review of the corrected immutable design before Ready or implementation.
