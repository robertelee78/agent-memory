# Routes lane report

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-routes`, detached baseline `5bd11582`. No Git writes, Python execution, subagents, or new crates.

## Implementation

Created:
- `src/cli/helpers/project_identity.rs`: exact baseline main parser for normalize/status/preview/apply/recover, using typed project_identity operations and command_result. Proposal read/decode failures enter the same operation envelope. Recover preserves baseline empty-string proposal truthiness.
- `src/cli/helpers/bank_state.rs`: inspect parser, --repo default '.', optional --path, typed command_inspect.
- `src/cli/helpers/tombstones.rs` (Main's scope addition): render --input, validate --file, filter --repo/--input over existing tombstones operations. All failures go through content-free tombstones::command_result.
- `tests/t2_routes.rs`: real `CARGO_BIN_EXE_am` processes in private, canonicalized support::World directories. No custom dependency providers or fake success paths.

Modified:
- `src/cli/helpers/mod.rs`: appended three helper registrations.
- `src/cli/table.rs`: project after developer; inspect after init with prefix ["inspect"]; tombstones between security-policy and validate-pack. Existing helper-only order otherwise unchanged.
- `tests/obligations/python-baseline-v1.json`: five previously unported current-bank obligations marked partial; four completed CLI obligations promoted from partial to ported; stale normalizer-only coverage note corrected.

Public APIs: each helper exposes only its standard `HELPER` declaration. No new domain APIs, traits, foundation changes, or new behavior deltas. Existing parser/input limits, UTF-8 policy and deterministic help layout remain inherited. Main-approved CLI naming convention keeps only project and inspect for scripts already in MEMORY_ROUTES; tombstones is helper-only. No project-identity/bank-state aliases.

## Coverage

Completed via real binary, exact ledger ids:
- `tests/contracts/test_project_identity.py::ProjectIdentityContract.test_normalization_strips_secrets_and_unifies_only_documented_equivalence`
- `tests/contracts/test_project_identity.py::ProjectIdentityContract.test_project_registry_stale_lock_recovery_is_bounded_and_read_back`
- `tests/contracts/test_tombstones.py::TombstonesContract.test_exact_five_field_format_roundtrips_without_personal_prose_or_generated_state`
- `tests/contracts/test_tombstones.py::TombstonesContract.test_sensitive_extra_fields_bodies_names_and_fingerprints_refuse_without_echo_or_mutation`

Also annotated: existing ported bootstrap contract with status/preview/apply/read-only-bank-initialization/replay coverage. Existing changed-registry/bank/remote/authority contract remains partial: new binary test covers changed remote and explicit authority selection/refusal; not claiming complete CLI replay of all bank/config/registry branches.

Six full current-bank CLI contracts are retained as real-binary tests gated `needs recovery_frontier registered`:
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_minimal_untracked_bank_survives_poisoned_then_removed_application_state`
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_all_six_tracked_contexts_retain_git_evidence_without_claiming_uncommitted_bytes_are_shared`
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_missing_git_still_reads_the_copied_portable_markdown_bank`
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_git_root_spelling_preserves_trailing_spaces`
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_busy_or_interrupted_bank_is_not_repaired_by_state_loss`
- `tests/contracts/test_current_bank.py::CurrentBankContract.test_git_inspection_ignores_ambient_repo_redirection_and_does_not_execute_fsmonitor_or_write_trace`

The first five unported current-bank rows were marked partial by ledger-mark.sh; missing-Git was already partial. Additional gated binary tests cover historical --path orientation/refusal (also needs comparison_view) and tombstone projection suppression/duplicate-ID refusal. Missing dependencies are not replaced with stubs. Gate removal and success-path execution require integration's genuine recovery_frontier/comparison providers.

Ungated coverage includes success/refusal for all project subcommands, parser missing/invalid arguments and help across all new subcommands, exact project-normalize stdout bytes, exact inspect missing-bank refusal bytes, tombstone render bytes and validate/refusal bytes, sensitive-input non-echo, no-write assertions, and typed file-read refusals.

## Verification

Platform: aarch64-apple-darwin, Darwin 27.3.0 arm64 (xnu-13432.60.57~99), rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1.

Observed after tombstones addition:
- `rustfmt --edition 2024 src/cli/helpers/tombstones.rs tests/t2_routes.rs` completed. Earlier formatting covered the project/inspect helpers.
- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: successful, zero warnings.
- `cargo test --offline --features dev-tools --test t2_routes`, three consecutive runs: each 9 passed, 8 explicitly dependency-gated, 0 failed.
- `/Users/robert.lee/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh - tests/t2_routes.rs`: marked 5 previously unported ids partial. Re-run with a completed-id list: 0 new ids. `target/debug/am-dev obligations mark --status ported --ids-file ...` promoted the four completed CLI obligations; temporary ids file removed.
- Required unwrap/expect/slice grep over the three new helpers: no matches.

An initial test expectation assumed recovery_frontier was reached without a bank; the real binary correctly refused earlier with the baseline canonical-bank-directory error. Corrected the test to the specified missing-bank refusal, not the implementation.

Final `cargo test --offline --no-fail-fast --features dev-tools`: successful on macOS after all additions, 1191 passed across 78 suites, 245 ignored, exit 0; wall time 586.45 seconds (raw evidence artifact://13767). No concurrent edits/builds occurred during this run.

The first whole-suite run completed in 611.78 seconds and only failed two t2_cli root-usage comparisons: the test executable had compiled its in-process table before Main added tombstones to the assignment, while the external binary had since been rebuilt with tombstones. Failure output differed only by that route. No source workaround was made; the final whole-suite run passed.

## Guide command inventory requested by Main

Ran `cargo run --offline --features dev-tools --bin am -- --json validate-pack --pack skills/agent-memory`. Validator returned valid=false, dispatch issue at `assembly/GUIDE.md`: `Command am context is not declared by the command surface` (validator stops at first issue).

Independently enumerated all `am <cmd>` references in shipped Markdown and compared against the actual binary's invalid-command alternatives. Every still-undeclared referenced command:
- **context** — assembly/GUIDE.md:69,88; maintain/cli/GUIDE.md:83.
- **transaction** — checkpoint/GUIDE.md:155; initialize/GUIDE.md:246; maintain/cli/GUIDE.md:93; writes/GUIDE.md:70,72,119.
- **refresh** — maintain/cli/GUIDE.md:89; sharing/refresh/GUIDE.md:76,93,94,123,150.
- **boundary** — maintain/cli/GUIDE.md:50.
- **semantic** — maintain/cli/GUIDE.md:88,109; sharing/semantic/GUIDE.md:82,83,107,110,120,265,266,378.

`semantic` is additional to Main's expected context/transaction/refresh/boundary remainder. All three Routes-owned command names are declared. No guide edits or out-of-scope route changes made.

## Accountability

`awa status --json`: managed=true, 164 managed issues, repository robertelee78/agent-memory, awa 0.10.15. Asked Main to retain cutover-story accountability ownership; no tracker mutation from this lane.
