# Ledger lane report

## Files

- Created `tests/obligations_ledger.rs`.
- Created `src/bin/am_dev/obligations.rs`.
- No ledger JSON or other test changes; no T0, error or Cargo edits.

## Integration

Main owns the bin target and dispatch. In `src/bin/am_dev/main.rs`, add `#[allow(dead_code)] mod obligations;` (the check helpers are used by the integration test), then route the `obligations` subcommand's remaining arguments to `obligations::run(args, Path::new(env!("CARGO_MANIFEST_DIR")))` and use its returned exit code. `args` is the slice after the `obligations` subcommand.

Public entry: `pub fn run(args: &[String], manifest_dir: &Path) -> i32`. Commands: `summary`; `mark --status <ported|partial> --ids-file <path>`. IDs are UTF-8, one per line; empty lines are ignored. Summary prints counts by status and contract file and the unported count. Mark refuses all unknown IDs before writing, recomputes status counts, and preserves the baseline one-space-indented JSON style; an unchanged encoding matches the committed ledger bytes exactly. The test imports the same module directly; no extra library module or trait is needed. No Python argparse surface or baseline Python obligations were assigned to this tooling lane.

The annotation scanner permits blank lines, any `//` comments, and attributes between annotation and function, per Main's instruction. The live ledger deliberately fails by default while A2 annotated IDs remain unported; `AM_LEDGER_REPORT_ONLY=1` prints findings without failing. Synthetic tests exercise each requested rule independently, and a fixture tests mark, idempotence and unknown-ID refusal.

## Declared deltas and remaining work

No runtime Python behavior is being ported by this developer-only tool. Annotation adjacency follows Main's explicit relaxation rather than the original stricter wording. Main must wire the module and update ledger statuses through the integration process. This lane does not mark baseline obligations itself.

## Checks

- `cargo fmt --check`: pass.
- `cargo check --offline --all-targets`: pass; seven existing warnings in `tests/t1_a2.rs`, none in owned files.
- `for i in 1 2 3; do AM_LEDGER_REPORT_ONLY=1 cargo test --offline --test obligations_ledger >/dev/null 2>&1 || exit; echo "run $i passed"; done`: three passes, five tests each.
- `cargo test --offline --test obligations_ledger live_ledger_is_consistent`: expected failure, 63 unported annotation findings in the snapshot; confirms default failure.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/bin/am_dev/obligations.rs`: no matches.
