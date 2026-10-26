# Publication timeout at the required growing-bank workload

## Observed failures

At frozen `e816844c4395858f3e1279f3847aafe557c84ac5`, the fresh macOS
12-session/10,000-lesson independent variation completed native capture and all
pending publication preparations, then its first `publish_memory.py land --v2`
exceeded the unchanged 300-second worker deadline. Command137 records exit null,
`TimeoutExpired`, 300.0349 seconds and empty stdout/stderr. The fresh required
Linux 24-session/10,000-lesson schedule subsequently failed on that same first
landing/deadline after all 24 pending writers were prepared. Neither is a pass.

Raw source-bound records remain under
`/tmp/agent-memory-v003/daily-use-v1-variation-12-10000-e816844/` and
`/tmp/agent-memory-v003/daily-use-v1-required-24-10000-e816844/` on the
qualification machine. The runtime bytes/modes were checked unchanged. The
controller collected the owned timed-out process group and cleaned its disposable
Worlds. Consequently the interrupted command's final bank/remote/index state is
unavailable; no preservation, corruption or successful publication is inferred
from its empty streams. Real operator banks/runtime were not targets.

## Mechanism and attribution limits

`land` intentionally performs a pre-retry replay, two source-bound semantic
checks, fresh repository-policy/index/source checks, normal push and post-push
replay. Those boundaries remain necessary. Within them, identical complete text
maps were repeatedly parsed and checked for format and safety.

A separately retained 10k diagnostic inspection observes 15 complete
`validate_files` traversals, 150,000 typed-record inherited-scope visits and 102,233
security scans. Instrumentation uses periodic cProfile snapshots; its cumulative
parent times are distorted by interruption/resumption and MUST NOT be presented
as native latency or a causal percentage. Static tracing independently identifies
the same repeated calls across snapshot, bounded Git-tree reads, semantic review,
merge/invalidation and replay. This supports a redundant-work mechanism, not an
exact allocation of the failed landing’s 300 seconds.

The first reduced diagnostic also failed because its new private `inputs/`
directory had not been created after native captures were deliberately removed.
That apparatus failure is retained separately. A subsequent direct inspection
completed, and continued diagnostics explicitly create the private directory.
They do not replace a fresh required workload.

## Bounded repair

The repair reuses successful complete-map format results and exact-text safety
results only inside explicit publication/semantic/refresh/recovery CLI operations.
Keys include all source bytes/paths and scanner flags; returned/stored values are
isolated copies. No observation, source hash, policy, identity, currentness,
semantic judgment, known-hold state or error is cached. Loaded definitions are
fixed during the operation. Entry/serialized-retention budgets evict or fall back
to normal checking rather than make memory unavailable.

Independent technical review found that whole format validation is not universally
pure: `file://` canonical-authority normalization resolves filesystem topology.
Every format hit therefore repeats router/header normalization before reuse.
Parsed-record output bypasses the memo. The optional recall catalog eagerly binds
the added validator module and conservatively falls back during an active operation
memo; ordinary cold/warm catalog operation remains separately tested.

## Qualification status

Development checks pass the 17 existing catalog methods and seven added validation
methods, including four public between-check mutation controls. They preserve
refusal for changed local memory, authority, original index and repository policy;
also cover changed file-authority topology, scanner limits/redaction isolation,
parsed output and operation/budget cleanup. These are development results, not
final immutable-source/platform or full-workload qualification by themselves.

Frozen repair `fc3a259407f9219c5920d44e9e602e02013250e7` subsequently passes
42 methods on each platform across the new seven-method suite, existing catalog,
semantic-sharing, refresh-v2, known-hold and quarantine suites. Test-only
`d5073e3dfdb47361ca8c5e9305e7f1b463aa7785` adds a deterministic retained-budget
eviction/oversized-result control that passes on both platforms. Its complete
portable runtime is identical. All source bytes/modes match before and after.
The memo's weight budget measures encoded values plus bookkeeping, not strict RSS.

A continued native public control over the retained 10k seed and a single authored
context change completes: inspection 29.558s, selected inspection 27.497s, page 27.203s,
review 44.711s, preparation 49.607s and landing 111.462s. Publication and feature/index/HEAD
preservation are verified; total control time is 290.838s. It is a diagnostic with a
different changed-learning frontier, not a paired equivalent of the failed
twelve/24-session admission workload or a whole-workload pass.

A fresh unchanged-controller six-session/128-lesson pilot at the frozen repair
also passes all 25 required events in 688.44s, including exact all-bank seed/admitted
learning preservation, positive holds, disclosure checks and expected feature
state. Independent code review approves the bounded static repair; it does not
adopt execution/latency or whole-story claims.

[Results, original failures, instrumented counts and independent reviews](data/2026-10-26-publication-validation/bindings.json)
retain their exact identities and qualifications. Fresh required-size schedules,
independent variations and complete platform gates are running at integrated
`81fec82c1619d20c12473d23e3fdf6706d52b641`. Reader-cost recheck and independent
whole acceptance remain necessary. No timeout is padded and no smaller/control
run establishes whole #99 acceptance or delivery.
