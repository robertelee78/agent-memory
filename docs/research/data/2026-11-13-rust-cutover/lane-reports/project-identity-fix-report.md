# Project identity independent-review fixes

## Scope and result

Worktree: `/opt/agent-memory-rust-project-fix`; assigned branch: `lane/project-identity-fix`; supplied immutable base: `06c186da42e356c211d19f42fe6057f2da96b92e`.

Addressed the three findings in `project-identity-review.md`. Implementation and regression edits are applied locally. Compilation, formatting, tests, independent review, tracker reconciliation, commit/push and integration remain parent-owned; no execution result is claimed here.

Changed files (complete list):

- `src/project_identity.rs`
- `tests/t1_project_identity.rs`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/project-identity-fix-report.md`

No other files, obligation ledger, dependencies, public APIs, registration contracts or foundation primitives were changed.

## Root causes and minimal fixes

### P1-1: nested bank roots

Discovery passed the entire repository-relative bank root to the single-component descriptor stat API. A root such as `docs/shared memory` was therefore rejected before its bank could be inspected.

Discovery now obtains the joined bank's parent and final filename with ordinary typed refusals for unavailable components, and calls the existing `stat_optional(parent, filename)`. That helper opens the parent's `DirectoryChain`, pins each ancestor without following symlinks, stats the one final component, and rechecks the held chain. Existing missing-parent/final-component behavior remains optional absence. Router inspection and reads retain their existing descriptor-based paths. No path-based `std::fs` trust fallback, new allocation/clone, input-derived panic, or weakened symlink policy was introduced.

### P3-1: symlinked lock refusal precedence

The early generic busy refusal previously intercepted every symlinked marker. It now applies only to transaction markers and owned locks. An unowned `.agent-memory.lock`, including a symlink, reaches the existing registered canonical-writer probe first and retains the exact refusal `Unknown or unsafe canonical writer marker; preserve for inspection`. Owned lock and transaction symlinks retain `Bank busy or interrupted; inspect/recover before resolving identity` before any marker read. The real canonical-writer probe and its Busy/Initializing kind mapping are unchanged.

### P3-2: busy-state consumer obligation

The existing busy-state test now registers the existing `WriterOperations` adapter backed by `bank_writes::OPERATIONS`; it no longer asserts missing registration as the consumer outcome. A `Writer` fixture explicitly uses mode 0600 for the exact malformed-marker refusal and mode 0644 for the exact unsafe-marker refusal, independent of umask. The same obligation now covers an actual symlinked bank lock, retained target bytes/link, and exact refusal through status and preview. Ordinary malformed/unsafe cases also exercise discovery and preservation.

## Exact regression names

New active tests:

- `configured_bank_roots_preserve_status_preview_apply_and_replay`: single-component `memory-bank`, depth-two `docs/shared memory`, and depth-three `docs/team/shared memory`; absent-bank status, read-only preview, apply, pending replay, initialized status, resolved replay, and existing-bank link preview/apply/replay. Snapshots cover read-only operations and replay, including registry and router preservation.
- `committed_nested_bank_roots_bind_without_explicit_configuration`: depths two and three with spaces, real committed routers, inferred location, status, bind preview/apply, resolved replay, and byte-preserving read-only/replay behavior.
- `nested_bank_roots_refuse_intermediate_symlinks_without_mutation`: both nested depths, intermediate directory symlinks, exact baseline settings refusal through status/preview, retained links, and unchanged repository/target snapshots.
- `owned_lock_and_transaction_symlinks_keep_busy_refusal_without_mutation`: owned lock, unowned transaction and owned transaction symlinks; exact generic refusal through status/discovery, retained links and target bytes, unchanged snapshots.

Corrected existing active test:

- `unknown_bank_symlinks_and_busy_state_preserve_sources`: real writer-operation registration, deterministic malformed/unsafe modes, and unowned symlinked-lock parity, while retaining the original unknown-router/registry-symlink cases.

Explicit pending real-dependency test:

- `nested_bank_roots_preserve_raw_recovery_identity`, annotated `#[ignore = "needs layout_recovery_observation registered"]`: retains the required depth-two/depth-three raw-recovery status identity and no-mutation assertions. Its bank-marker adapter calls the real existing `bank_observation::markers`. No layout provider is faked or substituted. The assigned source tree exports no `layout_recovery_observation` module; its real implementation/registration is required before enabling this test. The isolated-child launcher forwards `--include-ignored` for this exact case so an explicitly requested ignored run cannot silently skip its body in the child.

## Obligation status for parent

The unchanged exact baseline obligation annotation is:

`tests/contracts/test_project_identity.py::ProjectIdentityContract.test_unknown_bank_symlinks_and_busy_state_refuse_without_mutation`

Its existing mapped test remains `unknown_bank_symlinks_and_busy_state_preserve_sources`; the missing-wiring assertion has been replaced with real consumer assertions. No ledger ID/status adjustment is needed for that mapping solely because of these edits; execution/acceptance remains pending. New regression cases do not claim additional baseline obligation IDs. Raw nested recovery is explicitly pending the real layout-observation module, not claimed ported or verified. Existing partial obligations and release/CLI qualifications described in `projid-report.md` are unchanged.

## Limits and verification handoff

The implementation worker ran no Python, build, test, file-mutating formatter or Git command. It captured stdout-only rustfmt and applied formatting through anchored edits. Parent performed the following checks on Ubuntu 26.04, Linux 7.0.11 x86_64, rustc 1.94.1:

| Check | GNU | x86_64 musl |
| --- | --- | --- |
| `cargo fmt --check` | Passed | Same source |
| `cargo check --offline --locked --all-targets --features dev-tools` | Passed, zero warnings | All targets compiled during suite |
| `cargo test --offline --locked --features dev-tools --test t1_project_identity` (musl adds target) | 46 passed, 1 ignored, three consecutive runs | 46 passed, 1 ignored, three consecutive runs |
| `cargo test --offline --locked --no-fail-fast --features dev-tools` (musl adds target) | 475 passed, 0 failed, 3 ignored | 475 passed, 0 failed, 3 ignored |

The required panic-site scan found only the existing `expect("bundled project schema")` for a compiled-in asset, not an input-derived panic site. No source was changed after successful runtime proof.

Independent reviewer-model `AcceptProjectIdentityFix` (`anthropic/claude-opus-5-5:high`), separate from the fixer, accepted all three corrections after source inspection. It confirmed pinned parent-chain/final-name inspection, baseline lock refusal precedence, actual writer-operation assertions, and isolated per-case child processes. Review ran no tests. An optional P3 notes that the raw nested test remains ignored; this is explicitly retained until the real layout-observation provider exists, rather than claiming its test adapter qualifies that missing module.

The ignored `nested_bank_roots_preserve_raw_recovery_identity` is not counted as passing. Real held/raw integration, relocated-runtime qualification, macOS and aarch64 remain outside this proof. The existing two transaction dependency ignores also remain unchanged. No host configuration, network or credential dependency was added.

Evidence logs: `/tmp/agent-memory-project-identity-fix/` on `omp-beast`. Parent owns tracker updates, commit/push and integration handoff; the integration owner records this candidate and its independent verdict on #249.
