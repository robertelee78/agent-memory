# Worktree observations must not update shared tracking refs

A six-session independent-worktree workload intermittently reported an unavailable
v2 refresh inspection. Private diagnostic tracing binds the failure to the real
Git fetch in `sharing_recovery.observe`, rather than source selection, memory
corruption or the private-output writer. The original public result preserved
memory and feature work but hid that exact stage behind a general pending reason.

The source-frozen public-command reproduction at
`af7936b71861da93867fdab0ac0c1693cbf42b11` runs two observations from separate
worktrees sharing one clone after peer memory lands. It confirms that the old
fetch updates `refs/remotes/origin/main` in addition to its unique disposable
observation ref. The registered 16-method suite fails that invariant; the other
15 methods pass. Actual Git stderr records the unintended tracking update.
The original intermittent failure's Git stderr was discarded, so its exact
low-level exit diagnostic is not retrospectively claimed as captured evidence.

[Git's documented fetch mapping](https://git-scm.com/docs/git-fetch#Documentation/git-fetch.txt---refmapltrefspecgt)
explains why the explicit UUID destination was insufficient: a named remote's
configured fetch mapping can still update its tracking branch. That creates an
unnecessary shared update point for otherwise independent worktree observations.
The repair supplies an empty `--refmap=` so only the explicit disposable
observation destination is updated. It retains no-tags/no-FETCH_HEAD/submodule
controls, exact fetched-ref readback, before/after authority checks and owned-ref
cleanup. No retry conceals an authority change or loosens source qualification.

The new control requires both worktrees to observe the actual landed authority,
all existing remote-tracking refs to stay exact, all bank/code/index/HEAD bytes
to remain intact, and disposable observation refs to be gone. Supported-platform
qualification and the authored daily-use rerun must verify the repair; the small
control alone is not the full #99 acceptance gate. The retained original workloads,
private diagnostic trace and expected failing control remain under
`/tmp/agent-memory-v003/daily-use-v1-semantic-smoke-6-128-v6-diagnostic/` and
`/tmp/agent-memory-v003/tracking-ref-reproduction-af7936b/`.

Fetch failures also receive a typed, content-free `authority-fetch-unavailable`
result in refresh/semantic inspection, with an actual fetch-stage component and
safe connectivity/access/retry instructions. Raw Git stderr, remote values and
memory text remain private. An offline public refresh control checks that result
and preserves all original memory/feature state. This changes failure diagnosis,
not authority, retry policy, transport permissions or what may be published.
