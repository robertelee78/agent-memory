# Opus all-harness scope review

**BLOCKING: the wake limit covers Codex only.**

T3 limits fallback wakes only for **Codex Stop**: one per source-set fingerprint, and at most two per active turn. For Claude and OpenCode, the amendment says only that continuations are "shortened," "bounded" (by size) and "source-bound." The single rule against repeated waking ("never… keep waking indefinitely") applies only when tools are unavailable.

Claude's Stop hook can block repeatedly, and `stop_hook_active` only signals the re-entry. OpenCode's post-answer passes can also re-trigger. So no rule limits how many continuations those adapters may send while tools are available but assessment keeps failing (corrupt evidence, lock contention, a missing source). The 2048-byte cap limits each message's size, not how many messages are sent. T4 says "no repeated fallback loop," but no design rule defines that behavior for Claude or OpenCode.

**Fix:** State that the wake rule applies to all three adapters, using each one's native equivalent:

- at most one fallback wake per native source-set fingerprint, and a hard maximum of two per native turn/message lifecycle;
- once the limit is reached, remaining work stays visibly pending and the adapter stops blocking;
- the wake count is retained durably, scoped by native session/message IDs, before each wake is sent;
- for Claude, `stop_hook_active` alone is not enough to enforce the limit.

**Non-blocking notes:**

- Claude's UserPromptSubmit payload exposes no per-message ID; it provides session, transcript path and prompt. The "original message/source identifiers" wording should allow transcript-entry UUIDs or a local observation ID like T1's, so the clause is not silently unsatisfiable.
- Mark the frontmatter as amended, with a date, so the earlier acceptance isn't read as covering T3/T4.

Otherwise the T3/T4 additions are sound. I'll accept once the wake limit is generalized.

The specified all-adapter durable wake limit was added before task readiness.
