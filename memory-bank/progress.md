# Progress

## Works (v0.0.16, branch `fix/v0.0.16`)

- All lean commands: status, doctor, init, recall, remember, share, refresh,
  install/update/uninstall, session-start hooks for Claude, Codex and omp.
- omp is part of the default install when its agent directory exists; status
  and doctor list it with Claude and Codex and say when `am update` should set
  it up; uninstall removes only agent-memory's omp files.
- Install works on a fresh machine, after the lean `am uninstall`, and after
  the v0.0.14 `am uninstall` (its leftover receipt counts as no install).
- A plain `git pull` works after refresh and share with `pull.rebase` true or
  false (42-cell matrix in [LEAN-REPORT.md](../LEAN-REPORT.md)); the printed
  message names the step first needed in the cells where it cannot.
- `scripts/qa.sh` passes on macOS arm64; every command under 200 ms locally.
- v0.0.15 is published; v0.0.14 takeover verified from a real install and an
  offline QA fixture.

## Remaining

- Publish the signed v0.0.16 release; run `verify-release.yml` on it.
- `am update` from v0.0.15 on the operator's machine (two runs; see activeContext).
- A few real omp sessions on the cheapest model after a plain install.

## Known limits

- OpenCode keeps its skill link and activation block but has no session-start
  hook (the lean design covers Claude, Codex and omp).
- Install has no crash journal beyond the leftovers record; a half-moved
  runtime is restored only when the record proves it.
- omp named profiles (`OMP_PROFILE`, `~/.omp/profiles/<name>/agent`) are not
  detected; `--omp-agent-dir` covers them.

## History

Releases up to v0.0.14 and their evidence: [docs/releases/](../docs/releases/).
v0.0.15 and v0.0.16: [docs/releases/](../docs/releases/) and
[LEAN-REPORT.md](../LEAN-REPORT.md).
