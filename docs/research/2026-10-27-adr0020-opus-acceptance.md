# Independent Opus design acceptance

Requested model: opus. Reported model: claude-opus-5-5.
Isolated safe-mode text review; no implementation or memory operation claimed.

**ACCEPT**

Both prior blockers are resolved at the design level.

**1. Registration under lock contention.** Exclusive-create slot reservation is independent of the completion/inventory lock. Success requires durable readback, and partial or invalid slots count as pending. If durable registration fails, the input is blocked. Stop repeats its inventory before going quiet. Together these leave no window where an accepted input goes unrecorded or gets credited by an older acknowledgment. A source registered after Stop's final recheck stays pending and indexed, which is the correct outcome.

**2. Later discoverability.** The index reference is made durable before acceptance and rechecked after publication. Cleanup rechecks after removal and restores the reference if work remains. This two-sided recheck closes the orphaning race, provided the post-publication recheck re-establishes a missing reference. Make that explicit in implementation. Index capacity blocks new input rather than evicting old turns, legacy pending work joins the backlog view, and prompt-time and Stop surfacing complete the path.

**Non-blocking implementation notes:**

- **Crashed reservations can never be assessed.** A slot left partial by a crashed hook has no verifiable digest, so the recover-and-verify assessment path can never complete it. It will stay pending permanently. That is truthful and not silent, but specify the operator-visible resolution for it, such as an explicit "abandoned reservation" acknowledgment that does not claim assessment. This prevents permanent backlog noise.
- **Test the 128-slot exhaustion path.** Confirm that exhaustion both records the fault and blocks the input, rather than doing only one. The text supports both; T4 should assert both.
