I found **one concrete false-pass mechanism**, so this is not approved as-is. Both repairs check out, and the fix for the blocker is one line.

## B1 retry repair: confirmed
- **Lines 18–20:** the script refuses to run if any `commands`, `failure`, `result` or `attempt` file exists for either upgrade operation. A prepared World can therefore be used for only one of the two transitions, ever.
- **Line 21:** the World's live receipt must exactly match `legacy-receipt.json`. That file is written only after a successful 0.0.2 install from managed `main` (lines 70–71). Line 17 also requires `prepared: True`.
- **Line 23:** the `open('x')` create is the real atomic guard. It runs before any work and before the `try` block. A failed check before it leaves no files and changes nothing; a failure after it permanently blocks a retry.
- **Lines 26 and 30:** `fresh` and `prepare-legacy` refuse to reuse an existing output directory, so a prepared directory can't be overwritten by a fresh run.

## B2 uninstall repair: confirmed
- `install()` in `installation.py:869–880` writes the uninstall receipt with `verified: True`, `uninstalled: True` and `preserved` taken from the plan. Line 86 reads that receipt from disk, not from the command's output.
- An exit 0 with no real uninstall can't pass. The stale receipt would have no `uninstalled` key, and the "already uninstalled" branch can't be reached after line 83 read an active receipt.
- Requiring `preserved == []` also rules out the case where the source is kept "conservatively" (`uninstallation.py:87–88`).
- Line 87 checks with `lexists` that every non-activation target is gone. That covers the source, wrappers and links/copies. Line 88 additionally covers `am`.
- Lines 82 and 79 check the `--check` previews only for `check_complete` / `verified is False`, and never count them as applied outcomes. Applied success comes from lines 80 and 86.

## Blocker: Python's optimise mode turns every check into a no-op
**Trigger:** the script is run with `python3 -O`/`-OO`, or with `PYTHONOPTIMIZE` set in the operator's shell. That is the canary process's own environment, not the World's environment.

**What happens:** every check in the script is an `assert`. That includes the one in `run` at line 39 that checks the exit code. With asserts stripped, the script reaches line 90 and writes `passed: True`, `signed_publisher_verified: True` and `uninstalled: True` without checking anything. It even deletes the World at line 91.

- In `upgrade-via-cli`, a failed first transition (line 55) or a version reported as not authenticated (lines 57 and 78) would also be recorded as a pass.
- Because each prepared baseline allows only one attempt, the false result can't be re-run on the same World.

**Fix:** before line 13, add `if not __debug__: raise SystemExit('canary requires assertions')`. Converting the asserts to explicit checks would also work.

## Other points checked, no blocker
- **Downgrade/source scope:** `updating.current` (line 37) requires the recorded source path to equal the code root, and the release switch reuses that path. So the checks at lines 75–76 and 87 cover the old managed clone; there is no leftover legacy checkout.
- **Two-step `upgrade-via-cli`:** the second `update` goes through `release_transition`. Because both are 0.0.3, it must match the signed files and modes to the legacy Git ones (`updating.py:146–160`). The run notes `actual_public_default_install: False`, and line 57 checks that the interim state was not authenticated.
- **Cleanup scope:** `rmtree` deletes only the World, after the `/tmp/agent-memory-test-*` path, non-symlink and token checks (lines 15–16 and 91). Failures keep the World. The resumed World never calls `close()`.

## Limits of this static review (not blockers)
1. The script doesn't check the copied support files against the hashes in `source-before.json`; that is left to the operator.
2. `retained-world.json` has no integrity check. Its `paths` are checked against the World root at line 22, but its saved environment is reused as-is at line 24.
3. Activation targets other than the three seeded config files are covered only by the product's own readback plus `preserved == []`. The canary's line 87 skips activation targets.
4. The `security` fields from `--json version` come from code I wasn't given. I could only check them against `lifecycle-security.md`.
5. The interim receipt after the first `upgrade-via-cli` update is checked only for version and kind, not for `verified` or revision. The final checks cover the end state.
6. A possible false *failure*, not a false pass: restoring the config text exactly after two transactions depends on how 0.0.2 recorded its backups (`uninstallation.py:43–54`). Only a real run will show that.
7. No release, publication or run result exists yet; every claim above still needs the actual later executions.

**Verdict:** add the `__debug__` guard and this is approved for static scope, within the limits above.