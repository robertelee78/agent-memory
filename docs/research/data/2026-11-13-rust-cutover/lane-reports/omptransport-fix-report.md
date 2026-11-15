# omptransport fix report — fresh native install activation

Base: worktree refreshed byte-exact to integration `cc85cc37` (every tree file compared with `git -C rust-v015-int show cc85cc37:<path> | cmp`; 73 differing/missing files rewritten with a shell redirect; afterwards only one non-regular entry compared unequal — a committed symlink, already identical — and no local-only file remained). Diff against `cc85cc37`: `~/.local/state/agent-memory/research/rust-cutover/omptransport-fix.diff` (4 files).

## Defect 1 (reported): first install refuses "No installed Rust runtime receipt owns this executable"

Cause: `omp_boundary::InstallationBoundary` (the `installation::OmpBoundary` seam) gave every lifecycle hook the runtime root from `omp_boundary::runtime_root()` = `cli::pack_root()`, which resolves the *installed receipt* and requires the running executable to be that receipt's binary. `native_transition`/`native_activation` run before any receipt exists; `complete_stored_source_activation` runs after commit while the running binary is still the source/bootstrap binary, so it would have refused too ("This executable is not the installed agent-memory runtime").

Fix (`src/omp_boundary.rs`, approved):
- New `staged_runtime(source)`: the runtime a lifecycle source stages or owns = its absolute `pack` (canonicalised non-strictly; a release pack may not exist yet when planning). Baseline equivalent: the pack of the installer actually running, i.e. the one being installed. No `--pack` fallback; refuses "Lifecycle source does not name the runtime it stages" without a pack.
- `native_transition` / `native_activation` (seam): runtime root = `staged_runtime(plan.source)` — the plan's `source.pack` is the active pack the plan stages (development tree or the owned release runtime).
- `complete_stored_source_activation(state)` / `recover_native_binding(state)`: the `runtime_root` parameter is removed; each derives it from the committed receipt it already validates (`receipt.source.pack`, only after the early returns, at its single use).
- `runtime_root()` (= `cli::pack_root()`) remains only for operations run after installation (`am omp-boundary` route and stream).
- `tests/t2_omp_boundary.rs`: the 8 direct calls of the two changed functions drop the runtime argument (plus the now-unused `let runtime` in that one test). No assertion changed.

## Defect 2 (found by the same test, after defect 1): installed `--stream` refuses "Malformed target ownership receipt"

Cause: `omp_installation::read_receipt` (observer binding) validated target kinds against its own stale 7-kind list; a Rust v3 receipt also owns `command-link` targets (`~/.local/bin/am`, `agent-memory`), so every installed observer binding refused. Fix (`src/omp_installation.rs`, one check): accept exactly `installation::TARGET_KINDS` (the receipt schema's single definition), keeping the path-normalisation and identity requirements. The module's own unknown-kind refusal tests still pass. This file belongs to the ompinstall lane; the change is the minimal one on the fresh-install path you asked me to make pass — flagging it for your review.

## Acceptance

`tests/t2_omp_transport.rs::installed_stream_passes_observer_binding_and_reaches_real_transport` un-ignored and passing: real built `am` installed from a development source with `--presets omp --omp-agent-dir … --omp-peers-upgraded` (exit 0); installed `am omp-boundary --stream …` outside a bank → verified observer binding, finite worker started, `AM249 INACTIVE 0 …"inactive-no-bank"`, exit 0; forged `--artifact` refused (exit 1, no frames); installed `am omp-boundary --repo <git repo with bank> --list-pending` → real transport inventory `{"operation":"pending","pending":[],…,"inventory_complete":true,…,"canonical_writes_performed":false,"verified":true}`, exit 0.

## Commands (aarch64-apple-darwin, Darwin 27.3.0, rustc 1.99.1)

- `cargo fmt --check` → clean; `cargo check --offline --all-targets --features dev-tools` → 0 warnings.
- `cargo test --offline --features dev-tools --test t2_omp_transport installed_stream` → 1 passed.
- Whole suite `cargo test --offline --no-fail-fast --features dev-tools` → cargo exit 0; 73 `test result` lines, 1095 passed, 0 failed, 173 ignored (log `/tmp/omptransport-fix.log`).
- Files differing from `cc85cc37` afterwards: exactly `src/omp_boundary.rs`, `src/omp_installation.rs`, `tests/t2_omp_boundary.rs`, `tests/t2_omp_transport.rs`.
- Manual reproduction in a scratch world (`/private/tmp/am-omptx.*`, removed) showed the v3 receipt's `command-link` targets that triggered defect 2.

Process note (honesty): the diff file was written with the harness's Python eval tool calling `git show`/`diff` (the busy bash copy returned no output); no baseline Python was executed or inspected.
