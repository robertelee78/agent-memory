# Concurrent bank writer contract

The supplemental `concurrent-bank-writers` suite invokes the public bank writer,
transaction, recovery, sharing-boundary and publication scripts in disposable
Worlds. It uses actual competing processes and real Git repositories/worktrees
with a file-only isolated publication-provider fixture. It requires no account,
network service or live developer/client data.

Same-physical-bank contention is a defensive safety check. Normal coding
sessions use separate worktrees and branches; their shared logical memory is
reconciled through Git, not a cross-machine filesystem lock.

The suite verifies:

- One paused owner excludes four actual competing writer processes. Contenders
  fail without changing the owner's bytes or bank. Their stale requests still
  fail after the winner commits. Four explicitly fresh observed proposals fold
  in the remaining facts, leaving the original text and each fact exactly once.
- A concurrent bounded observer sees the complete old/new single-file bytes
  around replacement, with no sampled partial content. This is an actual
  process/filesystem observation, not a power-loss or arbitrary-syscall proof.
- An actual external editor invalidates a paused writer's target observation;
  its edit survives and no write receipt is fabricated.
- A live multi-file transaction owner cannot be stolen. After the owner is
  killed and reaped, explicit lock recovery leaves the transaction visibly
  pending. Public transaction finish restores the complete proposed bank, and
  the stale competing single-file proposal cannot overwrite it.
- Distinct worktrees simultaneously hold their own locks, retain one project
  identity and receive distinct external receipts. Sharing refuses each busy
  bank; its later manifests contain canonical memory paths only.
- An actual publication attempt refuses the live local lock without moving
  the isolated authority. A fresh post-release review/publication lands a real
  memory-only commit with no lock, temporary file or external runtime receipt,
  while unfinished feature files, index and branch history remain unchanged.

Conflict handling is bounded: fail on the live lock, refuse a stale observed
proposal, then require an explicit fresh observation. The test does not add an
unbounded retry loop or silently select a last writer. It records actual process
counts, busy/stale refusals, observation counts and the isolated landed commit in
the suite's raw stderr. Every child is collected or cleaned by its World, and
the outer worker retains its ordinary 300-second deadline.

The existing single-file and multi-file suites retain the broader recovery,
lost-response, malformed/foreign/live lock, changed read-set and artifact-safety
cases. This suite adds synchronized composition rather than relabeling those
earlier results. Cross-clone semantic conflicts and the full multi-developer
learning matrix remain separate release obligations.
