# Progress

## Works (v0.0.17)

- All lean commands: status, doctor, init, recall, remember, share, refresh,
  install/update/uninstall, session-start hooks for Claude, Codex and omp.
- omp is part of the default install when its agent directory exists; status
  and doctor list it with Claude and Codex and say when `am update` should set
  it up; uninstall removes only agent-memory's omp files.
- Install works on a fresh machine, after the lean `am uninstall`, and after
  the v0.0.14 `am uninstall` (its leftover receipt counts as no install).
- A plain `git pull` works after refresh and share with `pull.rebase` true or
  false, also with `pull.ff=only` (144-cell matrix in [LEAN-REPORT.md](../LEAN-REPORT.md)); the printed
  message names the step first needed in the cells where it cannot.
- `am recall QUERY` returns only the notes that have every query word (common
  words ignored; short words and numbers exact), best first, and says when it
  shows closest matches instead.
- `scripts/qa.sh` passes on macOS arm64, Linux arm64 and Linux x86_64; every
  command under about 200 ms locally.
- v0.0.15, v0.0.16 and v0.0.17 are published; v0.0.14 takeover verified from a
  real install and an offline QA fixture; omp works on the operator's machine.

## Remaining

- v0.0.18: the ten open items in that milestone; none started. Scope each one
  with the operator first (see activeContext).

## Known limits

- OpenCode keeps its skill link and activation block but has no session-start
  hook (the lean design covers Claude, Codex and omp).
- Install has no crash journal beyond the leftovers record; a half-moved
  runtime is restored only when the record proves it.
- omp named profiles (`OMP_PROFILE`, `~/.omp/profiles/<name>/agent`) are not
  detected; `--omp-agent-dir` covers them.

## History

Releases up to v0.0.14 and their evidence: [docs/releases/](../docs/releases/).
v0.0.15 to v0.0.17: [docs/releases/](../docs/releases/) and
[LEAN-REPORT.md](../LEAN-REPORT.md).
