**Verdict: not approved. There are two concrete blockers.** This is a static read of `public-release-journey.py` and the supplied support files only. Nothing was run and no files were changed.

## B1: `upgrade-legacy` never checks that the World is still at v0.0.2 before it runs
- **Lines 42–43 and 53:** on resume, `upgrade-legacy` only compares the consumer repo snapshot, then runs the public one-liner. Unlike `upgrade-via-cli` (line 46), it never checks that the receipt is still `0.0.2` / `managed` / `main`, and never compares it with `legacy-receipt.json` (line 64).
- **Trigger 1:** a first `upgrade-legacy` attempt succeeds at the one-liner but fails later, for example at line 67/68/69 because the legacy transition produced a wrong `kind`, a leftover `.git`/`memory-bank`, or wrong contents. A rerun on the same `--out` then does a v0.0.3-over-v0.0.3 reinstall, which can come out clean. It writes `upgrade-legacy-result.json` with `passed: True`, which hides the transition defect.
- **Trigger 2:** `upgrade-via-cli` fails after `old-process-update` (line 47), and the World is kept by line 82. Running `upgrade-legacy` against that same `--out` then reports a "legacy" upgrade from a World the old CLI already moved to v0.0.3.
- **Lost evidence:** on a rerun, `commands` restarts empty and line 31 overwrites the earlier attempt's `upgrade-legacy-commands.json`. A stale `upgrade-legacy-failure.txt` from the earlier attempt can sit next to a new `upgrade-legacy-result.json`.
- **Fix within scope:** before line 53, for `upgrade-legacy`, require that `receipt_path(w)` still matches `legacy-receipt.json` (version `0.0.2`, kind `managed`, same revision). Refuse to run if `upgrade-legacy-failure.txt` or `upgrade-legacy-result.json` already exists in `out`, and do the same for `upgrade-via-cli`.

## B2: `uninstalled: True` rests only on the `am` wrapper being gone
- **Lines 76–78:** the script runs `am uninstall` without `--json` and then checks only that `cli` no longer exists, plus the repo snapshot and config text.
- In `uninstallation.py:58–94` and `:145`, uninstall exits 0 with `verified` even when targets are kept back. Any target whose identity differs from the receipt is preserved (line 78), and then the source is kept as well (line 88). After uninstall, `installation.py:879–880` writes the receipt with `uninstalled=True` and that `preserved` list.
- **Trigger:** the v0.0.2→signed adoption records a skill-copy target identity that doesn't match. Uninstall then removes the unchanged wrapper and leaves the copy and the signed source behind. Exit is 0, `cli` is gone, `AGENTS.md`/`CLAUDE.md` text is restored, and the script reports `uninstalled: True`. This is the kind of transition defect the canary exists to catch.
- **Fix within scope:** run `--json uninstall` (or re-read the receipt afterwards) and assert all of these:
  - `verified` is true
  - `uninstalled is True`
  - `preserved == []`
  - every non-activation target path in the pre-uninstall receipt is gone, including `source`

## Checks that hold up (assuming B1 and B2 are fixed)
- **Signed authentication:** line 71 relies on the `version --json` security fields. Per `lifecycle-security.md:39–46`, those report `verified` only after a fresh check of the pinned signature and identity. Line 67 requires kind `release` and the exact `--expected-source`, and line 69 requires exactly the three signed-release entries.
- **v0.0.2 transition honesty:** lines 46–51 require old code to produce a `managed` 0.0.3 install reporting `not_established` and no signature check. Signed adoption is only accepted after the separate new-process update. `actual_public_default_install` is set to False for this path.
- **`--check` previews vs applied results:** lines 72 and 75 require `verified is False` from the previews. Applied claims come only from line 73 (`verified and not changed`) and from uninstall, which is subject to B2.
- **Isolation and preservation:**
  - Source, CLI and targets are confined to `w.root` (lines 55–56).
  - The repo snapshot, including `.git`/index (so staged work is covered), is compared before and after.
  - The three client config texts are compared exactly.
- **Scoping of resumed Worlds and cleanup:**
  - Resume requires a root under `/tmp`, an `agent-memory-test-` name, no symlink, and a matching token (lines 15–16).
  - `rmtree` runs only after success and after re-checking the token (line 79). On failure, the World is kept and reported.
  - A version mismatch fails closed: running `prepare-legacy` after promotion, or `fresh` before activation, fails at line 58.

## Limits (not blockers)
- `retained-world.json` env/paths are restored at line 17 without checking that they sit inside `owned`; only `root` is bound by the token. This is acceptable because the 0700 `out` directory is under operator control.
- The snapshot covers `.git/index` bytes, so a git index refresh during `am status` could cause a spurious failure. That would be a false failure, not a false pass.
- Isolation depends on env redirection. The script can't detect writes to absolute paths outside the World.
- I didn't check `source-before.json` against the files, and I didn't verify the `--source` checkout's `scripts/` helpers used by `bank`/`bind`.
- None of this replaces the actual public runs after v0.0.3 is published.