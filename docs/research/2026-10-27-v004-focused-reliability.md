# v0.0.4 focused reliability evidence

This is a bounded qualification component for #99, not a whole-story verdict or
proof that 24 sessions and 10,000 lessons completed. The operator set a 10–15
minute budget for each test and moved work to make that stress workload practical
to #209 in v0.0.5. The previous 24/10,000 Linux test timed out during refresh;
a later three-worker run was stopped before completion. Neither is a pass. The
previous independently reviewed 12/1,000 semantic/recovery and 24/1,000 and
12/10,000 core cohorts are described with their exact scopes and limitations in
[the v0.0.3 qualification report](2026-10-26-v0.0.3-qualification.md).

The focused tests below ran against the portable package and test fixtures in
commit `69278b8e4ee6b86838eae1af3d5cf31dc4b16a6e`. The [source binding](data/2026-10-27-v004-focused-reliability/source-binding.json)
records the 340 fixture/package Git blobs checked against the source archive on
both macOS and Linux. The macOS archive was checked again after the tests. These
are isolated local Git/CLI fixtures, not hosted CI or production data. Each
suite was a separate `python3 -B -m unittest` invocation with `PYTHONPATH=tests`
(or `tests:tests/contracts` for the two individual v2 cases).

| Scenario | macOS | Linux | Evidence |
| --- | --- | --- | --- |
| Separate worktrees, 3 clones, 2 developers, publication, scoped peer recall and preserved code | 1 pass, 132.515 s | 1 pass, 22.812 s | `macos-compatible.log.gz`, `linux-compatible.log.gz` |
| Conflicting memory, detecting developer and unrelated work | 1 pass, 104.194 s | 1 pass, 18.649 s | `macos-conflicts.log.gz`, `linux-conflicts.log.gz` |
| Interrupted refresh, lost response, offline return and preserved code | 2 passes, 64.238 s | 2 passes, 12.112 s | `macos-recovery.log.gz`, `linux-recovery.log.gz` |
| Memory erasure survives later ordinary code merge and replay | 1 pass, 66.804 s | 1 pass, 13.910 s | `macos-merge.log.gz`, `linux-merge.log.gz` |
| Simultaneous publishers reconcile duplicate learning | 1 pass, 90.591 s | 1 pass, 16.489 s | `macos-race.log.gz`, `linux-race.log.gz` |
| v2 refresh imports independent safe peer learning while holding unsafe material | 1 pass, 33.868 s | 1 pass, 6.409 s | `macos-v2-refresh.log.gz`, `am99-linux-v2-refresh.log.gz` |
| v2 publication shares independent safe learning while holding unsafe material | 1 pass, 53.706 s | 1 pass, 8.412 s | `macos-v2-publish.log.gz`, `am99-linux-v2-publish.log.gz` |

Raw command output is retained under [the evidence directory](data/2026-10-27-v004-focused-reliability/).
The legacy cross-clone and recovery suites use the v1 compatibility interface;
the two v2 suites exercise current public sharing operations. Fixture agents
supply authored judgments; fixture harness labels are not live Claude, Codex or
OpenCode hook tests and these cases do not measure general semantic accuracy.
The independent review must assess the source bridge to the final integration
candidate and the complete v0.0.4 acceptance, including actual hook behavior.
