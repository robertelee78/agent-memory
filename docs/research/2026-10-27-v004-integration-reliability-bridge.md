# v0.0.4 integrated reliability bridge

This evidence bridges the earlier [focused reliability qualification](2026-10-27-v004-focused-reliability.md)
to the integrated runtime candidate at commit `69fc75d44f76921c6c984a6d4016cc03dd1d7cf1`.
The candidate includes the bounded-bank and diagnostic fixes, so the relevant
sharing/recovery behavior was retested on both supported operating systems. It
is not a claim that this commit is the final release source; the final
integration must establish whether these tested package and fixture bytes remain
unchanged or run the relevant checks again.

Seven independently bounded invocations ran on each platform against clean Git
archives of the exact commit. Before and after each host's run, the runner
compared all 343 Git blobs and executable modes in the portable package, test
fixtures, repository memory fixture and test gate against the commit. Each
command receipt records its exact executable and argv, environment overrides,
working directory, host/platform and Python version, start time, duration,
exit status, and SHA-256 of the captured stdout and stderr. The verbose unittest
output names each executed method. The longest single invocation took 181.044
seconds, under the operator's 10–15 minute limit.

| Scenario | macOS | Linux | Scope |
| --- | ---: | ---: | --- |
| v2 safe peer refresh with withheld unsafe source | 31.787 s | 6.865 s | 1 test each |
| v2 independent publication with withheld unsafe source | 51.171 s | 9.181 s | 1 test each |
| Interrupted import, lost response and offline recovery | 66.558 s | 13.098 s | 2 tests each |
| Later ordinary code merge and erased-memory replay | 76.106 s | 14.712 s | 1 test each |
| Simultaneous competing publishers | 113.494 s | 17.452 s | 1 test each |
| Three clones, two developers and separate worktrees | 181.044 s | 24.435 s | 1 test each |
| Cross-developer contradiction and independent work | 136.850 s | 19.163 s | 1 test each |

All fourteen commands exited zero and all sixteen named tests passed. The
[retained evidence](data/2026-10-27-v004-integration-69fc75d/) contains the
source manifest, the runner, both hosts' before/after source observations,
per-command receipts, compressed raw stdout/stderr, and an integrity inventory.
`python3 -B docs/research/data/2026-10-27-v004-integration-69fc75d/verify.py /path/to/repo`
verifies the evidence against Git and checks receipt/log identities and test
results. The verifier is an evidence-integrity check, not an independent
acceptance verdict.

The v2 cases use authored fixture judgments and exercise actual public sharing
operations. The other tests exercise the v1 compatibility interface, real
local Git fixtures, and simulated harness labels. None establishes live
Claude, Codex or OpenCode hook invocation, general semantic accuracy, production
network behavior, or a successful 24-session/10,000-lesson stress run. The
original timeout and stopped rerun remain non-passing evidence; making the
complete stress workload practical is tracked in #209 for v0.0.5.
