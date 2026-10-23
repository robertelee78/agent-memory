# Landed-memory refresh

The portable `scripts/refresh_memory.py` entrypoint supports `inspect`, `prepare`
and `apply`. The [operation guide](../../skills/agent-memory/sharing/refresh/GUIDE.md)
defines automatic turn-boundary use and pending/recovery responsibilities.

Inspection obtains actual base/local/landed sources for the existing semantic
review interface, using an explicit last-observed commit, a disposable verified
cursor hint, or actual Git common ancestry. Preparation takes exactly `proof`
and a current-recall `recall` request and returns one observed transaction
proposal. The `refresh_evidence` extension binds that exact proof, project,
recall request and actual local branch/revision to the complete reviewed result.

Application rechecks sources before writing and under the owned bank lock. It
uses the existing recoverable per-file observed-write protocol; it does not
stage the original index, commit/rebase/merge code, overwrite a directory snapshot
or discard unpublished learning. Completed receipts do not reapply bytes. An
unfinished source-bound operation requires rollback and fresh source review;
a verified completed receipt can clean up and resume actual readback.

A fresh post-import observation must still match the reviewed incoming memory
before imported recall can claim current shared freshness. New/offline authority
remains explicit pending work even if the local transaction completed. Relevant
recall preserves provenance, conditions and hard currentness gates. Disputed
paths/dependencies are excluded from current context while independent imports
continue; retained local disputes do not become winners.

The optional content-free cursor binds the actual finished receipt/checksum and
an observed authority head. It is a disposable baseline hint, not canonical
memory or authenticated authority. Missing state can be reconstructed from Git;
damaged state remains preserved. A later import/local correction survives an
older retry, and ordinary later code integration is a separate branch workflow.

All tests use public copied-pack commands, temporary Git repositories and
isolated homes/config/state. The two-profile learning-loop test evaluates
capture, admission, memory-only Git publication and current recall in a stale
second clone; harness names are attribution fixtures, with no live client use.
