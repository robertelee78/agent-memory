# Docs lane report: Rust cutover (worktree rust-v015-lane-docs @ 5764442c)

## Files changed

- README.md: install (POSIX `sh` bootstrap, three targets, curl and ssh-keygen required, Git for the runtime, no interpreter or Cargo), reinstall from v0.0.14 and older, dropped `--remote`/`--ref`, development source needs `target/release/am`, update, out-of-band recovery recipe (verified `am-<target>` plus `recover`, signer taken from `install.sh`), diagnostics fallback, development checks (`cargo test --features dev-tools` per triple, `am-dev obligations summary`), 18.19 probe sentence.
- docs/contracts/: assembly, bank-router, bank-transactions, bank-usability, bounded-resume, checkpoint, consolidation, developer-attribution, governance, independent-local-bank, initialization, installation (rewritten for the Rust bootstrap and signed-only channel), installed-layout, lifecycle-faults, lifecycle-model, lifecycle-security, manual-checkpoint, memory-publication, memory-refresh, multi-machine-learning, observed-writes, portable-lifecycle (run and CI section), project-identity, retrieval, runtime-diagnostics, runtime-uninstall, runtime-updates (signed release-v2 only), scope-binding, security-policy, semantic-sharing, sharing-boundary, sharing-recovery, tombstones. All 82 Python references are replaced with `am <route>` commands, `src/<module>.rs` modules or the Rust test targets.
- docs/testing/: README.md (cargo test, obligations ledger, baseline gate kept as obligations, `tests/support/world.rs`, Rust smoke recipe), daily-use-v1.md, incidents-and-guides.md, public-release.md, recall-qualification.md, omp-native-qualification.md. observed-guide-examples.json is untouched.
- docs/releases/v0.0.15.md: new top section "Rust-only runtime (ADR-0029)" with the declared deltas (no interpreter, three targets, reinstall from v0.0.14, managed channels dropped, out-of-band recovery, added formats) and port status. The previous content now sits under "Superseded Python candidate record (historical)", with its headings demoted from `##` to `###`. No in-repo links point at those anchors (checked).
- skills/agent-memory/schemas/record.json: `x-local-invariants` now reads "The Rust record contract (src/records.rs, am record-contract) additionally checks …". The checks it lists were verified in src/records.rs.

Schema pin: no test pins the record.json bytes. records.rs and model_reference.rs use `include_str!` on it. `cargo test --offline --features dev-tools --test t1_a2 --test t2_cli` ran after the change and passed (t1_a2: 80 passed; t2_cli: 24 passed).

Incident: my first edit to record.json went to /opt/agent-memory (the main checkout) because I used a relative path. I reverted it with an exact inverse edit at once. `git status` in /opt/agent-memory is clean.

## Remaining `python3|Python|\.py\b` hits (all intentional)

Adoption mentions:
- contracts/installation.md:39, a Python-era managed checkout is adopted read-only
- contracts/runtime-uninstall.md:8, a Python-era receipt refuses with re-bootstrap guidance
- contracts/runtime-updates.md:7, Python-era receipt v1/v2 refuses
- testing/omp-native-qualification.md:79, adopting a Python-era v2 receipt re-requires `--omp-peers-upgraded`
- releases/v0.0.15.md:31, 37, 61 (adoption, plus "superseded Python candidate" evidence)

Historical:
- releases/v0.0.15.md:65 onward (heading and intro of the superseded record), and lines 106, 114, 204, 248–251, 323, 520, 557, 593, 596–598, 726, 743, 766, all inside that section
- testing/recall-qualification.md:144, the retained historical protocol file `/tmp/…/206-refresh-50000-v5.py` (SHA-pinned)
- testing/recall-qualification.md:221, labels extracted from `semantic-controller-v5.py:19–36`

The ledger path `tests/obligations/python-baseline-v1.json` is cited often. It does not match the grep, which is case-sensitive.

## Statements not verified by execution

- `cargo test --features dev-tools --target {x86_64,aarch64}-unknown-linux-musl`: not run (macOS host). The Cargo.toml comment implies musl debug tests run.
- The v0.0.14 updater's behaviour against a Rust release ("cannot update themselves") comes from the ADR and design; it was not observed. The docs mark it as required before publication.
- README recovery recipe and `releases/latest/download/am-<triple>` URLs: built from the install.sh constants and the release namespace. No Rust release is published to try them against. The `sed` extraction of the signer was tested against install.sh.
- "A separately verified `am` binary runs recover/status/doctor/uninstall/purge-data": inferred from direct-mode dispatch in `src/cli/mod.rs::entry`. I did not run it against a broken install.
- runtime-uninstall: "a valid receipt with a missing runtime can still remove the remaining targets" was translated from the baseline "missing source" wording and not exercised.
- runtime-updates: the umask-independent reuse and outside-sentinel reconciliation prose is retained baseline wording, not re-verified.
- Coverage lists that now point at Rust tests come from the baseline: t2_consolidation (13 partial in ledger), t2_governance (5 partial; uses `AGENT_MEMORY_FAULT_STAGE` on the real binary), t2_diagnostics (11 ported / 14 partial).
- bank-transactions: I removed the baseline claim "no installed CLI prerequisite" rather than assert it.
- Behavioural prose not tied to running instructions or module names was not re-audited in the contracts.

## Gaps: docs describe behaviour the Rust code lacks

1. `am refresh` and `am boundary` are not in `src/cli/table.rs`: `memory_refresh` and `boundary_assessment` are unported. `tests/wiring-pending.txt` has `turn_check::register_memory_refresh waits for memory_refresh`. Yet `am --help` lists "refresh", the pack CLI guide documents `am refresh` and `am boundary`, the README says "`am boundary` invokes the existing refresh protocol", and contracts/memory-refresh.md (now `am refresh`) describes it.
2. All 15 evaluation controllers are unported, so the daily-use, public-release, recall-qualification and omp native smoke procedures cannot run. The guides now say so.
3. The incident runner, guide-example capture and rule-map checker have no Rust tool (test_incident_evidence: 7 unported). The two incident regressions exist in Rust and are partial.
4. Test gate: there is no Rust equivalent of the test-gate-v1 receipts, the `tests/contracts.json` registry, shards and cells, the 300-second per-suite deadline or matrix aggregation. No Rust test reads `tests/lifecycle-matrix.json` or `tests/shared-learning-matrix.json`. lifecycle-faults, portable-lifecycle and multi-machine-learning keep these as release obligations.
5. Ledger: 340 ported, 570 partial, 952 unported of 1,862. Suites cited in these docs:
   - unported: bounded_resume 3, installed_layout 3, independent_local_bank 3, enrollment_progress 27, layout_recovery_legacy 1, and the frontier pending-repair case
   - mostly unported: omp_boundary (49 of 51)
   - mixed: installer 15 unported / 6 partial, runtime_purge 9 unported, runtime_update 7 unported
6. CI (`.github/workflows/contracts.yml`) runs `cargo test --locked` on *-linux-gnu runners without `--features dev-tools`. The musl jobs only build. This does not match the "release triples, dev-tools" test instruction.
7. The `src/bin/am_dev/main.rs` usage text says `--binary TRIPLE=PATH (four times)`, but `release_distribution::TARGETS` has three.
8. The install.sh `--help` text still advertises `--remote URL --ref BRANCH`. The binary refuses these with `MANAGED_GUIDANCE`.
9. The signed `command_surface` declaration (`src/cli/surface.rs`) carries baseline script names (`installer.py`, `admit_learning.py`, …) in its `script`/`helper` fields. This puts `.py` identifiers into the release payload.
10. `tests/incidents.json` still names Python suite paths (outside this lane).
