**Verdict: no blocking defect in the measurement method. The 3,600 s observation can keep running without a restart. Five things need fixing now, and none of them touches the workload. #209 stays Designing, the numeric target is still undecided, and this review approves nothing for the operator or for implementation.**

This was a read-only review. I ran no commands, so I could not recompute any SHA-256 values.

## 1. The controller diff

**Verified by reading the files:**
- The original `209-source-clean/scripts/evaluations/daily_use.py` has 782 lines and the variant has 792. That matches the diff exactly: one hunk adds 10 lines and the other two change one line each.
- The grep sites agree line for line, with a +10 offset after line 17. These include `ThreadPoolExecutor` at 713/723, 721/731 and 750/760, the outer `except`/`finally` at 776–782 and 786–792, and the `finally` at 533/543.
- The only remaining `timeout=` in the controller is the public deadline (line 73) plus the new `communicate(timeout=10)`.
- The run's inputs are unchanged:
  - Product source, argv and the default four workers.
  - The `--recovery-controls` and `--semantic-scenarios` flags.
  - `require_scenario_events`, the seed and feature oracles, and the forbidden-marker assertion.
- `World.run`'s cleanup watchdog is `timeout+30` (`tests/world.py`), so it scales to 3,630 s without any edit.
- `PreserveFailedWorld` only takes effect when an exception propagates through the `ExitStack` (line 658). On success it calls `super().__exit__`, so cleanup is unchanged. I found no other temporary directory or cleanup context in the controller or in `publication_fixtures.py`.
- Every `.py` file in `/tmp/agent-memory-v15-readiness/` has a hyphenated name and can't be imported, so the script-directory entry on `sys.path` can't shadow modules.
- The supervisor checks SHA `8ddbc61d…` before launch. Disposable Worlds still use the allowlisted environment, and public helpers still run with `-I`.

**Not verified:** the byte hashes `988c81…`, `8ddbc6…` and `80cf2e…`, and byte equality outside the three hunks. Running `cmp`/`sha256sum` after the run would close this.

The run is progressing: `all-captures-admitted` for round 1 is at 1,375.1 s, against 1,333.4 s in the original.

## 2. Measurement apparatus defects (fix now, no restart)

**D1. Work keeps running after the first failure, and this matters far more at 3,600 s.**
- Each phase uses `with ThreadPoolExecutor(...)` with `executor.map`. The pool's exit waits for every task already submitted.
- So after the first exception, the remaining sessions' public commands keep running, each with up to 3,600 s. The original's commands 621 and 622, which timed out after 589–592, are exactly this effect.
- Consequences:
  - Terminal wall time can include hours of drain that isn't workload cost.
  - The other sessions' later failures don't appear in the traceback.
  - The retained Worlds show the state after the drain, not the state at the first failure.
- **Fix:** before results go to the parent, define censored time as the end of the first raising command (from `commands.json` row number and seconds). Report drain time separately. Label `retained-worlds.jsonl` contents as post-drain state.

**D2. The stop paths can't preserve evidence.**
- The supervisor's host-safety branch sends SIGTERM to the controller's process group. Each public command runs in its own session (`World.run`, `start_new_session=True`), so those commands are not signalled. They keep running for up to 3,600 s, which is the opposite of what a disk-safety stop is for.
- Python's default SIGTERM handling also skips all `__exit__`/`finally` code. That means no `retained-worlds.jsonl`, `outcome.json`, `failure.txt` or `source-after.json` would be written.
- SIGINT would instead trigger the D1 drain.
- **Fix:** write down the manual procedure now. World roots can be recovered from each row's `environment.AGENT_MEMORY_TEST_ROOT` in `commands.json`. Owned command groups appear in `resources.jsonl` under `owned_processes`. If this path ever fires, record the outcome as `INTERRUPTED`/`INVALID` with missing evidence listed as missing.

**D3. Evidence is kept under `/tmp`.**
- `World` hard-codes `dir="/tmp"`, and both attempts' raw directories are under `/tmp`. That doesn't satisfy the plan's "owner-only immutable" retention clause (line 187); the directories are 0700 but writable.
- I couldn't check this host's tmp-cleaning policy because `/etc/defaults/periodic.conf` wasn't readable.
- The retained Worlds hold private fixture data: the test token, withheld values and the credentials directory. No retention period, size bound or disposal owner is declared for them.
- **Fix:** after the run, copy both attempts and any retained Worlds to durable owner-only storage with a hash manifest. Declare the retention period and who disposes of the data.

**D4. The source binding checks the clone against itself.**
- Neither run passed `--source-binding`, so `expected=source(root)` is computed from the clone itself. `source-before.json` (1387/192) is therefore true by construction. It does require git-clean status and HEAD 33c95, which is useful, but it isn't an independent check.
- **Fix:** after the run, compare `out/source-binding.json` with the pre-run `209-source-clean-binding.json`. Also confirm that `out/controller.py` hashes to `8ddbc6…` and the clone's `daily_use.py` to `988c81…`.

**D5. The run doesn't meet the plan's own evidence list (line 187).**
- `start.json` and the preflight omit the Git version, memory and filesystem type. These can be recorded now; they are host constants.
- File-backed refresh outputs still aren't stored as blobs, and on success they are deleted along with the World. This is the gap the plan itself identifies at line 47.
- `RUSAGE_CHILDREN` misses orphaned processes, so its CPU totals are only approximate.
- **Fix:** declare all of this as an explicit limitation of a cost characterization, not as qualification evidence.

## 3. Integration rules that apply regardless of start, compared with the approved milestone contract (e78eee1)

The plan already covers:
- Paired builds from the same integrated candidate.
- The #244 comparison-v2 cutover, with migrations reviewed by the #209 acceptance owner and a new baseline required if the workload changes.
- The final run on the integrated release candidate.
- No hard dependency on #206 or #244.

Gaps:
1. **No overlapping performance runs on one host.** The milestone forbids this. The plan only cites #206's statement that no heavy run is active or planned. It should bind #206 and any other heavy run to stay off this host until `am209observe3600` finishes.
2. **Judgment-v2 declaration.** The milestone says "Each plan must explicitly declare whether semantic-sharing-judgment-v2 changes." The plan only records #244's position. It needs #209's own declaration: the measurement doesn't change it, and any future design must state its position.
3. **Shared-module rules buried in the historical sections.** The rules that no cache or memo may skip #244's frontier guards (including in `memory_refresh` and `sharing_frontier`), and that the `validation_memo` lifetime must be integrated jointly with #206, appear only in sections A–G. Those sections are now context only, so the rules need restating as active rules.
4. **Acceptance session and Linux host.** The plan should record the exact acceptance session and immutable candidate before each acceptance run. It should also name Linux host availability (the colima endpoint was down) as an execution resource gate for cell L.