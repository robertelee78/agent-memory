# v0.0.15 lifecycle fixes and retention parity

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-lifefix`, assigned detached base `a2d05a0f`.
Baseline authority (read only): `/opt/agent-memory-worktrees/v015-release`, `78eaf70`.
No Git/tracker writes, Python execution, new crates, subagents, or new product seams.

## D1 — update discovers the binary's own version instead of latest

Root cause: `src/updating.rs` passed `ReleaseAccess.first_manifest_url` to `release_distribution::probe_with`. That URL is intentionally compiled-version-pinned for bootstrap installation. The lifecycle fixtures also used latest for that same field, masking the production defect. Baseline `updating.py:289-308` invokes `release.probe()` at line 293; `release_distribution.py:376-387` resolves latest before acquiring the signed, version-pinned payload.

Fix: add `ReleaseAccess.latest_manifest_url`, defaulting to the exact latest-manifest URL used by Rust `release_distribution::probe()`. Ordinary update uses this second field. Bootstrap `installation::stage_release` still uses `first_manifest_url`; install/re-run-bootstrap remain compiled-version-pinned. All three struct-literal fixtures now distinguish pinned bootstrap from latest update; the re-run-bootstrap helper also pins the requested binary version. The installation fixture now actually publishes both manifests. The bootstrap pin test additionally checks the production latest URL. `docs/testing/public-release.md` records the two URL contracts and qualification requirement.

Regression: `tests/t2_lifecycle.rs::update_check_finds_published_latest_without_following_the_bootstrap_pin` uses `tests/support/distribution.rs` with the actual built `am`, actual shipped pack, a throwaway signing key, and authenticated file-channel manifests. It installs 0.0.15/all/copy, retains that pinned manifest, publishes 0.0.16 as latest, then requires `update --check` to report an update with zero world mutations. It also executes the installed signed command and verifies the receipt remains 0.0.15. Existing lifecycle update tests now also distinguish the endpoints rather than silently aliasing them.

Red proof: before changing the update call, `cargo test --offline --features dev-tools --test t2_lifecycle update_check_finds_published_latest_without_following_the_bootstrap_pin -- --exact --nocapture` exited 101 at the `update_available` assertion (0 passed, 1 failed; artifact `15110`). After fixing the call, the check passed its availability/preservation assertions, but an additional installed `--version` apparatus assertion refused because this throwaway-key fixture is not production version evidence. That nonessential apparatus assertion was replaced by installed `--help` plus exact receipt-version readback; no product version behavior was changed.

## D2 — declared baseline rollback retention, not an uninstall defect

Main explicitly confirmed that `78eaf70` remains authoritative and directed retention to stay unchanged if parity holds. Product cleanup has therefore **not** been added: deleting these files would remove intentionally retained evidence.

### Why the genuine v0.0.14 capture has no backups

`~/.local/state/agent-memory/research/rust-cutover/v014-state/captures/all-copy-then-uninstall/manifest.json` did not successfully run uninstall. Its observations are:

- `[0]`: `installer.py install --remote ... --ref main --mode copy --presets all`, exit 0.
- `[1]`: argv `["uninstall_runtime.py"]`, exit **2**, empty stdout, stderr: `usage: uninstall_runtime.py [-h] {uninstall,purge-data} ...` and `error: the following arguments are required: command`.

There are zero `.agent-memory-` entries because this captures a fresh installation, not a completed uninstall. The named scenario is misleading; no inference about older-version cleanup can be drawn from it. This was obtained by read-only `jq`; no baseline execution occurred.

### Baseline commit and output contract

`installation.py:1338-1374` constructs `.agent-memory-<transaction>-<index>` and `.agent-memory-backup-<transaction>-<index>` beside every changed target that had a previous identity. The snapshot is added to `receipt.backups` with `kind: pre-activation-snapshot`; the stage is also added with that previous identity, without a kind. Both have `available: true`. At uninstall activation, `:1391-1407` renames the old target into its rollback stage. `:1417-1421` also records the old receipt backup. `:1422-1439` commits the receipt and returns, without deleting any stage or snapshot. There is no commit cleanup to port.

`uninstallation.py:58-63,110-142` defines `preserved` as client/runtime targets excluded from removal because ownership or edits require preservation, **not** the receipt's intentionally retained rollback backups. `:215-217` returns `install.install(plan)` plus this separate `preserved` list. Thus `verified: true`, `preserved: []`, and existing receipt-listed rollback nodes are the baseline result for a successful clean uninstall. Rust `uninstallation.rs` and `installation/transaction.rs` follow the same fields, naming, ordering, and retention. Existing `docs/testing/public-release.md:31-35` explicitly distinguishes these intended backups from unexpectedly retained runtime targets.

Read-only examination of `/private/tmp/agent-memory-test-qcopy.ZNCFul` found 13 current uninstall actions, 26 current transaction stage/snapshot nodes, and 27 current-transaction receipt backup records (the 26 nodes plus the old receipt). The receipt contains 35 backups in total because earlier installation backups are inherited, exactly as baseline `:1317` specifies. Its `uninstalled` is true and `preserved` is empty.

### Dangling symlink contract

Baseline `installation.py:191-204` copies symlinks using `os.readlink` into `symlink_to`, preserving the literal target, never dereferencing or rewriting it; `:121-122` fingerprints literal targets even when dangling. Native rename preserves the stage link identically. Accordingly the OpenCode plugin's two rollback links keep their old runtime hook target and dangle after removal. Rust `installation/filesystem.rs::copy_now` does the same descriptor-relative readlink/symlink operation.

The Rust command aliases are also symlinks to the owned `runtime/bin/am`; both their stage and snapshot links keep exactly that original target and become dangling. Python-era command launchers were regular wrapper files (`installation.py:479-496`), not command aliases, so it would be false to claim that 78eaf70 had those same command-alias symlinks. Their different original node type is the already-designed Rust executable/alias cutover, not a retention mismatch. Backup names and treatment of whichever node type was owned remain identical.

### Interrupted update recovery

Baseline `_rollback` (`installation.py:1068-1105`) restores original targets, durably marks `rolled-back`, and explicitly returns every existing action stage and backup in `preserved_backups`. It does not delete the pre-activation runtime snapshot; later recovery of a rolled-back journal (`:1191-1199`) re-verifies without pruning it. Rust retains the same evidence and reports it. Removing the runtime backup after write 29 would be a baseline-parity regression.

Parity coverage (actual signed-channel fixture and real `am`, no mocks):

- `signed_uninstall_retains_receipt_listed_rollback_nodes_including_dangling_links`: all/copy signed install followed by public `am --json uninstall`; exact persisted/output receipt equality; empty public and receipt `preserved`; exact stage/snapshot naming and backup records; every previous identity retained; no unexpected hidden transaction nodes; six dangling links (plugin plus two aliases, stage and snapshot); committed recovery leaves retained evidence unchanged.
- `signed_update_write_29_recovery_reports_the_retained_pre_activation_backup`: signed install/update, actual `update-write-29-after` exit fault, public recover, original receipt/targets restored, exact ordered `preserved_backups`, and retained runtime snapshot containing the original actual binary.

These are parity guard tests, not red/green tests for a nonexistent D2 product change.

## Verification

Platform observed: `aarch64-apple-darwin`; Darwin 27.3.0, kernel `xnu-13432.60.57~99/RELEASE_ARM64_T6050`; rustc 1.99.1 (`58cf8f9c3`, 2026-10-13), LLVM 23.1.1. No Linux qualification claimed.

Completed: `cargo fmt`; `cargo fmt --check`; `cargo check --offline --all-targets --features dev-tools` with zero warnings. Focused, final core, and directly affected shared-fixture consumer targets are green, including all three consecutive runs detailed below.

## Files authored

- `src/installation/seams.rs`
- `src/updating.rs`
- `tests/support/distribution.rs`
- `tests/t2_installation.rs`
- `tests/t2_lifecycle.rs`
- `docs/testing/public-release.md`
- This external report.

No changes to uninstall, transaction retention, filesystem foundations, wire formats, or command surface.

### Focused runs and fixture corrections

- Latest-manifest regression: green, 1 passed (11.16 s).
- Committed uninstall retention parity: green, 1 passed (10.92 s).
- Interrupted update/write-29 recovery retention parity: green, 1 passed (14.50 s).
- The first retention test attempt exposed only a test-code mistake: the JSON value type compares object insertion order, whereas durable receipts use the baseline sorted-key codec and public results preserve insertion order. Receipt/identity assertions now use the existing installation structural equality function; all fields and values remain checked.
- The initial full lifecycle run passed delivery (36/36), installation (65/65), and 47 lifecycle tests, with two pre-existing ignored purge tests. One old negative bootstrap fixture failed because it implicitly depended on bootstrap following latest: its old and next stand-in binary bytes are identical, and the corrected pin legitimately selected the old manifest. The negative case now explicitly models a misdirected pinned endpoint (a newer signed manifest presented to an older compiled-version binding) and still requires the exact version-mismatch refusal. This preserves the negative guard while keeping normal fixture endpoints honest. Final consecutive gates restart after this fixture correction.

### Final core gates — three consecutive green runs

After the last fixture correction, `cargo fmt` and `cargo check --offline --all-targets --features dev-tools` completed successfully with **zero warnings**. The corrected bootstrap refusal test passed (1/1), followed by three consecutive executions of:

```sh
cargo test --offline --features dev-tools --test t2_lifecycle --test t2_installation --test t2_delivery
```

| Target | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| `t2_delivery` | 36 passed, 0 failed (1.17 s) | 36 passed, 0 failed (1.25 s) | 36 passed, 0 failed (1.14 s) |
| `t2_installation` | 65 passed, 0 failed (67.23 s) | 65 passed, 0 failed (66.06 s) | 65 passed, 0 failed (63.14 s) |
| `t2_lifecycle` | 48 passed, 0 failed, 2 ignored (88.44 s) | 48 passed, 0 failed, 2 ignored (90.05 s) | 48 passed, 0 failed, 2 ignored (89.01 s) |

Both lifecycle ignores predate this work and require security/recovery-frontier registration; neither was added or narrowed. All three new signed-channel tests ran and passed in every core gate execution. The entire final command completed with exit 0 (474.72 s).

### Shared fixture consumers and library — three consecutive green runs

All direct signed-channel fixture consumers discovered in `tests/*.rs`, plus the library, passed three consecutive executions of the following command (exit 0; full output artifact `15140`; 751.77 s total):

```sh
cargo test --offline --features dev-tools --lib \
  --test c_test_cache_update_recovery --test c_test_doctor_fix \
  --test c_test_download_diagnostics --test c_test_gaps_adoption \
  --test c_test_gaps_lifecycle --test c_test_installed_layout \
  --test c_test_lifecycle_lost_responses --test c_test_public_cli \
  --test c_test_runtime_update --test c_test_update_discovery_gaps \
  --test c_test_update_mutable_public --test c_test_update_parallel_inventory \
  --test t2_migration --test fanout_b_omp
```

Counts below were identical in runs 1, 2, and 3. All ignores were pre-existing registration/cutover dependencies; no ignore was added or changed.

| Target | Passed per run | Failed | Existing ignored |
| --- | ---: | ---: | ---: |
| library | 21 | 0 | 2 |
| `c_test_cache_update_recovery` | 6 | 0 | 0 |
| `c_test_doctor_fix` | 3 | 0 | 3 |
| `c_test_download_diagnostics` | 6 | 0 | 0 |
| `c_test_gaps_adoption` | 1 | 0 | 10 |
| `c_test_gaps_lifecycle` | 7 | 0 | 9 |
| `c_test_installed_layout` | 3 | 0 | 1 |
| `c_test_lifecycle_lost_responses` | 1 | 0 | 1 |
| `c_test_public_cli` | 2 | 0 | 1 |
| `c_test_runtime_update` | 5 | 0 | 1 |
| `c_test_update_discovery_gaps` | 3 | 0 | 1 |
| `c_test_update_mutable_public` | 1 | 0 | 6 |
| `c_test_update_parallel_inventory` | 2 | 0 | 0 |
| `fanout_b_omp` | 49 | 0 | 207 |
| `t2_migration` | 4 | 0 | 12 |

## Disposition

D1 is fixed, with observed red-before/green-after signed-channel coverage and corrected endpoint fixtures. D2 is closed as intended baseline retention, not a product defect; both committed removal and interrupted-update recovery now have explicit signed-runtime parity guards. No retained evidence was deleted. All assigned gates completed successfully on the stated macOS target. Main owns integration and tracking.
