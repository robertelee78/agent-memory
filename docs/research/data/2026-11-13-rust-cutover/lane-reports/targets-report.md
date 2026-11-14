# Targets lane report

Operator decision: exactly three signed targets; Intel macOS is unsupported. No Git writes, Python, network, or sub-agents.

## Changed files and lines

- `install.sh:48,122-127,299` — help, host mapping, fixed unsupported-target JSON guidance.
- `tests/t2_bootstrap.rs:20,516-548,559-594` — three supported host cases; Darwin/x86_64 exact refusal.
- `.github/workflows/contracts.yml:25-31` — remove Intel macOS runner.
- `src/release_distribution.rs:79-83` — three-target verifier and builder shared list.
- `src/bin/am_dev/release.rs:78-82,275-283,1425-1428` — three-binary requirement messages; publisher test derives its asset count from the target list.
- `skills/agent-memory/schemas/release-v2.json:19-27` — exact three keys.
- `tests/t2_delivery.rs:271-287,1192-1277` — reject removed manifest target, missing/extra builder targets, and assert three inspections/digests.
- `src/installation.rs:118-138` — unsupported Intel macOS no longer recorded as a supported release triple.
- `docs/design/v0.0.15-rust-cutover.md:3,21-22,29,61` — operator decisions and remove awaiting-confirmation statements.
- `docs/research/data/2026-11-13-rust-cutover/lane-brief.md:15` — supported-target list corrected.

Search additionally found historical target mentions in `docs/research/data/2026-11-13-rust-cutover/lane-reports/bootstrap-report.md` and `delivery-report.md`, retained as lane reports; historical `docs/research/data/2026-11-05-ssg-runtime/linux-portability/discovery-20261106/release.yml:77` references an Intel macOS runner, not the removed target triple, and was retained. ADR 0029 has no target-specific list to change.

Public API: `release_distribution::TARGETS` now has array length 3. No registration traits, CLI declarations, foundation changes, or obligation ledger changes. No ported/partial/deferred obligations added. Declared delta: Intel macOS is no longer an accepted bootstrap or release target.

## Verification

Platform: `aarch64-apple-darwin`, Darwin 27.3.0 arm64, rustc 1.99.1 (58cf8f9c3 2026-10-13).

`shellcheck install.sh` and `dash -n install.sh` passed.

`cargo test --offline --features dev-tools --test t2_bootstrap`: 29 passed. `cargo test --offline --features dev-tools --test t2_delivery`: 34 passed. Initial `cargo test --offline --features dev-tools --no-fail-fast` found the publisher unit test's old 12-asset count; fixed it to derive the expected count from the target list. The corrected whole-suite run passed: 631 passed, 28 ignored.
