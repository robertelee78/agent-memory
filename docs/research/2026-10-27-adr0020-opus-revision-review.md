**Not ACCEPT. Two release-blocking gaps remain.**

**1. Registration under lock contention can silently lose a source (T1)**

Registration uses the same nonblocking flock as completion and Stop inventory. On contention it must "preserve state and return concise pending". But a UserPromptSubmit hook fires once, and nothing re-registers that observation later. The fault indication is only required "whenever safe state access permits". Contention is exactly the case where that access is unavailable, so no fault gets recorded.

The likely trigger is the late-steering-versus-completion race you list in T4. The holder finishes completion, Stop takes the lock, every recorded observation has a receipt, and Stop goes quiet. The omitted steering message is concealed, which is the failure T1 says it prevents.

Required fix, either of:
- Make registration lock-free and append-only. For example, exclusive-create one owner-only file per observation UUID, bounded per turn. Have Stop's locked inventory enumerate those files. The lock then serializes only completion recording and inventory.
- Or require registration to always durably record something outside the lock, such as an exclusive-create fault sentinel. Stop must treat that record as pending.

T4 should explicitly test registration contending with completion and Stop, and assert that Stop is not quiet afterward.

**2. Pending work does not carry across turns (T1/T3)**

The v2 namespace is scoped per turn. After the two-wake cap, remaining work "stays visibly pending without another block". Interrupted or cancelled work also stays pending. Nothing defines how the next turn, or a resumed or new session in the same checkout, discovers pending observations or faults from earlier turns.

As written, Stop for turn N+1 inventories only turn N+1. Unassessed sources from turn N become orphaned, and later turns can look fully complete. That contradicts "MUST NOT be silently cleared or called assessed" in spirit.

Required fix:
- Define a bounded checkout/session-level index of turns with pending observations or faults.
- Specify where those turns are surfaced: prompt guidance, quick check, or Stop. Stop should report them without blocking.
- Specify how a later turn may assess them truthfully without reassigning their source.
- Add T4 cases for cap exhaustion, then next turn, and for interrupt, then a new session.

The other prior concerns are adequately addressed: per-observation coverage, the receipt/snapshot conjunction, and fallback bounding.
