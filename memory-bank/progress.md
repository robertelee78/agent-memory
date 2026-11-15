# Progress

## Works (v0.0.15 lean, branch `lean/v0.0.15`)

- All lean commands: status, doctor, init, recall, remember, share, refresh,
  install/update/uninstall, session-start hooks for Claude, Codex and omp.
- `scripts/qa.sh` passes on macOS arm64 and in CI on Linux x86_64 and arm64;
  every command under 200 ms locally, hook 2–15 ms.
- v0.0.14 takeover verified from a real install and an offline QA fixture.
- Live omp recheck: 4 of 4 sessions asked before saving a memory.

## Remaining

- Publish the signed v0.0.15 release; run `verify-release.yml` on it.
- A few real omp sessions on the cheapest model.

## Known limits

- OpenCode keeps its skill link and activation block but has no session-start
  hook (the lean design covers Claude, Codex and omp).
- Install has no crash journal beyond the leftovers record; a half-moved
  runtime is restored only when the record proves it.

## History

Releases up to v0.0.14 and their evidence: [docs/releases/](../docs/releases/).
v0.0.15 summary: [docs/releases/v0.0.15.md](../docs/releases/v0.0.15.md) and
[LEAN-REPORT.md](../LEAN-REPORT.md).
