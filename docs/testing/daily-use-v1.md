# Manual daily-use qualification

The [approved workload design](../design/v0.0.3-daily-use-repair.md) owns the
requirements. `scripts/evaluations/daily_use.py` executes its finite authored
fixture through public capture, comparison, discovery, admission, review,
publication, refresh and governance commands. Real local Git remotes, three
independent clones, separate worktrees/branches, two developer IDs and four
harness labels model the shared repository. Fixture judgments assert known
scenario meanings; this controller is not a model semantic-quality evaluation.

Run from a clean, trusted source checkout with Python 3.11+ and Git. Choose a
new private output directory outside the checkout. The source remains unchanged;
all homes, banks, client configs, repositories and remotes are disposable Worlds.
No real credentials, installed runtime, live bank or hosted CI are needed.

```sh
python3 -B scripts/evaluations/daily_use.py --root . --out /tmp/am-daily-required-12-1000 --sessions 12 --lessons 1000 --recovery-controls --semantic-scenarios
python3 -B scripts/evaluations/daily_use.py --root . --out /tmp/am-daily-required-24-10000 --sessions 24 --lessons 10000 --recovery-controls --semantic-scenarios
```

Independent size/session variations exercise the compatible two-round and
recovery core separately:

```sh
python3 -B scripts/evaluations/daily_use.py --root . --out /tmp/am-daily-variation-12-10000 --sessions 12 --lessons 10000 --recovery-controls
python3 -B scripts/evaluations/daily_use.py --root . --out /tmp/am-daily-variation-24-1000 --sessions 24 --lessons 1000 --recovery-controls
```

Every session prepares an actual source-bound pending publication before the
first push in each round. Ascending then reverse landing order exercises stale
plans. Each refreshed bank must preserve every ordinary admitted record's exact
metadata/body, as well as successful qualified peer recall. The semantic schedule
adds same-file compatible edits, contradiction and conditional duplicate cases,
clarification/consolidation, pruning and tombstones, explicit caller code
integration, and withheld replacements with useful independent learning. Fault
controls exercise offline return, lost responses, interruption/recovery,
lost cursor/pages, stale authority, missing/duplicate review acknowledgements,
changed framing closure, ambiguous/rewritten bases and an actual old consumer.

The output retains exact source bindings, commands, private inputs/results by
content hash, feature/index/HEAD histories and outcomes. Exit 0 means this one
cohort completed; it does not accept the whole story. A failed or continued debug
run must remain identified, never replaced by a smaller pass. Whole acceptance
requires both full schedules, variations, the separate #98 retrieval evidence,
and independent review. The 48-session/50k combined experiment remains optional.
