# OpenCode native boundary qualification — 2026-10-27

Executable: `/Users/robert.lee/.opencode/bin/opencode`, version 2.1.3. Runs used
`run --standalone`, disposable HOME/config/data/repository, copied existing native
credentials, candidate plugin files and the configured Baseten Kimi-K3 model.
No production service was restarted or user repository changed. Credential files
are absent from these retained artifacts. Source byte maps bind each trial;
normal runs are separate native messages, not claimed as same-turn steering.

## Observed results

- The final-source trial completed two identical-text ordinary messages with
  distinct native source IDs: exit 0 in 61.409 and 23.881 seconds. Both answered
  “The result is five.” and completed no-learning assessments. The continuation
  repeated the substantive answer; it did not replace it with bookkeeping.
- Explicit deny-all native permissions produced a no-tools turn in 27.194
  seconds, exit 0. It preserved the arithmetic answer and explicitly retained
  the memory pass pending because source-window/ack tools were unavailable.
  No publication or completion was claimed for that source.
- Hard interruption of the isolated standalone process occurred after native
  enrollment. Its original session/source IDs survived. A later run in that
  native session answered the substantive requests and completed its own new
  source assessment while leaving the interrupted source pending. The command
  reached its 75-second deadline after tool activity; this is partial resume
  evidence, not a clean native CLI completion PASS. Final metadata contained
  three completed sources and two pending sources (no-tools and interrupted).
  Each source had at most one recorded wake attempt; no repeated wake loop was
  observed. Interruption was process termination, not a claimed graceful API abort.
- An unsafe external state directory prevented enrollment and blocked the native
  request: exit 1 in 0.873 seconds. A focused repeat returned exit 1 in 0.800
  seconds. OpenCode JSON stdout exposed only `UnexpectedStatus: 500`; stderr
  was empty. The server log retained the actionable inner message:
  “Agent Memory: native source enrollment failed; preserve the input and inspect
  pending_sources.” The client-facing loss of detail is a native host limitation;
  this evidence does not claim that the default CLI shows that guidance. No
  verified public upstream issue or documentation reference is asserted here.

## Provider control and limits

The initial GLM-5.3 trial emitted step-start events without an answer and reached
170 seconds; its second run was stopped rather than repeat the stall. A separate
GLM-5.3 control with no Agent Memory plugin also reached a 40-second deadline.
Kimi-K3 then completed ordinary turns. This separates the observed GLM stall
from a demonstrated memory-hook cause; it does not establish the provider’s
precise root cause. Failed trials remain failed, not passing qualification.

The focused native/queue/legacy ledger contracts passed 19 tests in 8.690
seconds, covering durable pre-context enrollment, native backlog pagination,
original-session recovery, wake deduplication/two-attempt cap, nonblocking FIFO
refusal, namespace revalidation, external-state enforcement and v1 compatibility.
The public JSON artifacts retain source hashes, bounded event summaries and
explicit outcome limits. Full controllers/raw logs remain private under the
`am-v008-opencode-live-*` proof directories; no credentials are copied to Git.

This is no-learning qualification. It does not prove durable admission,
publication, imported freshness, universal semantic completeness, or production
activation. Runtime UI/tool events cannot be promised invisible.
