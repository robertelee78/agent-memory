# Exact owned large-marker compatibility

Delegated correction on integration base `8c6adfbcaee58e2720899857d367f4644826df73`.
The #244 owner retains integration and independent Opus review; #209 owns the
narrow correction/regression and its later controller qualification. The user's
approval to fix the repeated-page bottleneck is not a reason to weaken recovery.

## Failure and correction

The six-session/128-seed controller stopped before its first capture during
public scope authority rebind. Its real regular0600transaction-marker-v1 was
13,828bytes because legitimate complete base hashes scale with bank inventory.
The private recovery-frontier reader rejected it before checking the already
supplied exact transaction witness. Existing v0.0.14 behavior permits known-owned
large ordinary markers while refusing unknown markers.

`recovery_frontier.marker_bytes` now accepts an optional exact byte witness only
for the canonical transaction marker. Without it, the4096-byte bound is unchanged.
With it, the read is bounded by the witness length within the existing finite
private-operation cap and MUST equal those bytes exactly. Existing no-follow,
nonblocking, regular/single-link, stable opened/named/parent checks remain.
`_observe_index` and `require_scope` pass their existing transaction witnesses;
no new caller-supplied JSON ownership channel, schema/size-based exception,
lock-only bypass or larger unknown/sentinel cap is introduced.

The first correction covered `_observe_index` but missed `require_scope`.
The composed six-session/128-seed controller consequently passed authority
rebind and all six initial captures, then failed its first admission apply.
Independent Opus review identified the same missing witness propagation.
The second correction preserves the witness at that scope check, covering
admission final readback and interrupted transaction finish rather than changing
the unknown-marker policy. Both failed pilots remain evidence, not passes.

Interrupted governed admission before its recorded receipt still requires
rollback followed by fresh comparison; finish is deliberately refused. The
large-bank recover-finish check therefore interrupts after the durable receipt,
the existing supported lost-response recovery boundary. An initial smoke used
the earlier marker-only boundary and correctly received that policy refusal;
it is retained as a failed test setup, not a product regression or a pass.

## Actual verification

- Existing public bank eligibility/large transaction apply, exact retry,
  interrupted finish, rollback and unknown-marker refusal suite:10methods pass,
  including a new frontier regression proving a real large owned marker preserves
  the original frontier while absent/short/same-length-changed witnesses and a
  copied large sentinel refuse without changing the bank/feature/evidence.
- Existing recovery-frontier comparison-v2/pending hold/rollback suite:4methods
  pass. No errors, failures or skipped methods in either suite.
- Actual public authority-rebind smoke over48real typed fixture records passes,
  preserving every record's exact bytes, project identity, feature/index/HEAD and
  removing the transaction marker after success.
- Actual public admission smoke over48existing typed records passes both direct
  apply and durable-receipt interrupted recover-finish. The interrupted marker
  is5685bytes. Exact prior learning and feature/index/HEAD survive; cleanup
  removes the marker and public admission replay reports an already-applied,
  verified historical admission without performing new writes.
- New permanent large-bank admission regression passes both direct and
  interrupted-finish subcases. A separate full admission-suite attempt exceeded
  the disposable runner's300-second limit; it is not reported as a passing
  full-suite result. The14methods above qualify the first correction; the
  focused regression and actual public smoke qualify the second caller.

These are source-bound local development checks, not full #209 controller,
24-session/10k, Linux or signed-release acceptance. The original failed controller
and captured marker evidence remain private; no source guard or workload was
shrunk to obtain the passing result. Integration owner reviews this standalone
commit before combining the checked-page export and resuming qualification.
