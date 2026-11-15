# sharingboundary lane report

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-sharingboundary`, initial integration snapshot `a725b97`, refreshed to Main-approved `b6ae1e34` for every changed non-owned file. No Git/GitHub writes, Python execution, subagents or new crates.

## Owned changes

Created `src/sharing_boundary.rs`, `src/admission_navigation.rs`, `src/cli/helpers/sharing_boundary.rs`, `tests/t2_sharing_boundary.rs`.

Append-only additions: `src/lib.rs` exports both modules in alphabetical order; `src/cli/helpers/mod.rs` exports the helper; `src/cli/table.rs` adds `am sharing` in baseline order; `src/cli/mod.rs::register_providers` registers the existing sharing_recovery boundary seam. Removed its satisfied row from `tests/wiring-pending.txt`. Updated `tests/obligations/python-baseline-v1.json` using the prescribed ledger tools. No foundation, error-kind or registration-trait changes.

## Public API and integration

`sharing_boundary` exposes baseline-named public functions:

- `inspect(repo: &Path, owned_lock: Option<&[u8]>) -> Result<Value>`
- `prior_deletions(root: &Path, prefix: &str, indexed: &IndexEntries, current: &BTreeSet<String>) -> Result<BTreeSet<String>>`
- `IndexEntries = BTreeMap<String, (String, String, String)>`: repository-relative indexed path to mode/blob/stage.
- CLI envelope: `command_result(Result<Value>) -> lifecycle::CommandResult`.

`admission_navigation` exposes `caption(&Value) -> Result<&str>`, `placement(&Value, index: bool) -> Result<&Value>`, `navigation_value(&Value) -> Result<&Value>`, `route(path, plan, scope, state) -> Value`, and `apply(files, navigation, learning, history, metadata, stamp) -> Result<Value>`. Inputs and output maps use the shared JSON Value. The optional Python parameters (`index=False`, route state `active`) are explicit Rust parameters. No source payloads are generated or replaced by navigation.

Per Main's explicit instruction, sharing_boundary calls `sharing_recovery::tree(..., false)` and `sharing_recovery::command(..., false, limit, false)` directly and imports its metadata bound: no second recovery seam. With Main's later approval, public `sharing_boundary::SharingBoundary` implements sharing_recovery's existing `SharingBoundary` trait by forwarding to `inspect`; it is registered in the CLI composition root and its pending row removed. There are no new pending wiring rows.

CLI: `am sharing --repo REPO`; helper metadata retains baseline `sharing_boundary.py` / module `sharing_boundary`, without an executable Python alias. Baseline indentation and refusal envelope are preserved.

## Implementation completeness and deltas

Both baseline modules are fully ported, including prior deletion ownership proof, complete index/ignore inventories, authority branch precedence, safety redaction, source/config/index rechecks, body-free observation digest, navigation schema/byte limits, explicit indexes, duplicate-route refusal, and preservation of existing introductions, omitted-context bytes and source payloads. No baseline function or branch is intentionally deferred.

Declared deltas: strict UTF-8 and shared bounded descriptor reads; nonordinary/hardlinked/changed sources and symlinked ancestors are refused rather than followed. Invalid Git stage frames refuse rather than reproducing Python unpacking exceptions. Missing providers propagate as typed refusals rather than proving deletion absence. Canonical output sets and touched navigation maps are deterministic; existing file order remains unchanged and new files are appended in sorted touched-path order (Python set iteration was unspecified). Foundation bounds and finite Git child deadlines are inherited from sharing_recovery/identity_git. No allocation-heavy subprocess argv per canonical path; inventories query one bank prefix.

## Contract coverage

Implemented contract assertions cover default boundary roles/readiness/non-mutation; authority branch precedence; ignored/untracked/runtime-artifact blockers; unsafe content and interrupted bank refusal; unsafe/duplicate configuration; symlink/alias/two-root and invalid-branch refusals; secret-shaped remote redaction; changed configuration invalidating prepared writes; and custom-bank self-authentication refusal. Ten tests are dependency-gated as detailed below; no green end-to-end readiness claim is made before recovery_frontier registration.

Partial cross-module obligations:
- nested configured bank: complete boundary, validation, exact observed-write prefix, writer inspection and security-audit assertions are written but recovery_frontier-gated; bank_state CLI inspection belongs to its route owner.
- unsafe explicit/discovered bank prefix: sharing refusal, redaction and source preservation tested; initialization and scope-binding helper outcomes are outside this module's port.
- unstaged canonical deletion: complete boundary proof and excluded-artifact negative-control assertions are recovery_frontier-gated; an additional isolated real-Git test directly exercises prior_deletions successfully, including mode/stage/blob mismatch and dangling-symlink negative controls without index mutation. Semantic publication proof belongs to publication/semantic_sharing.
- discovery meaningful navigation: native topic placement, exact captions, map introduction/source preservation and payload non-generation tested directly; admission transaction/replay belongs to admission/discovery.
- discovery navigation refusal: namespace and source/route collisions tested directly; trace/proposal substitution checks belong to admission/discovery.

Additional tests cover bounded/safe captions, exact navigation schema, duplicate/new-index bounds, unknown parent, existing topic-index introduction preservation, exact route serialization, capacity details, public help and direct prior-deletion proof. Discovered nested-bank readiness and executable/unmerged index tests are written but frontier-gated.

Cross-module baseline cases in test_sharing_boundary.py not claimed by this lane: nested admission receipt replay/cleanup; empty nested initialization/checkpoint; interrupted nested multi-file recovery; lost-setting initialization/prepared-write preservation; fresh clone initialization/binding; ignored/unknown-schema router initialization; tracked-router deletion/locator bound; damaged working/indexed router initialization; damaged untracked context initialization. These exercise other modules and remain their owners' obligations, not omitted sharing_boundary branches.

## Verification and ledger

Platform: aarch64-apple-darwin, Darwin 27.3.0 arm64; rustc 1.99.1 (58cf8f9c3, 2026-10-13), LLVM 23.1.1.

Executed: `rustfmt --edition 2024 src/sharing_boundary.rs src/admission_navigation.rs src/cli/helpers/sharing_boundary.rs tests/t2_sharing_boundary.rs` (success); `rustc -Vv`; `uname -a`; `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/sharing_boundary.rs src/admission_navigation.rs` (no matches, exit 1).

Final completed checks: `cargo fmt --check` clean; `cargo check --offline --all-targets --features dev-tools` clean with zero warnings; `cargo test --offline --features dev-tools --test t2_sharing_boundary` green three consecutive times, each 13 passed / 10 ignored / 0 failed. The ignored tests use exactly `#[ignore = "needs recovery_frontier registered"]`, approved by Main: bank_observation::observe unconditionally needs its unported provider. Fixtures create public identity before rendering the bank, avoiding an unrelated binding transaction; test-library registration uses Once because existing turn_sources registration is set-once.

GitHub/Vox: read-only `awa status --json` reports managed repository robertelee78/agent-memory, 164 managed issues; `vox agent status --harness omp` reports Ready node omp-work, bound room kh57aq2zwdss. Main is the only Git/GitHub writer and owns tracker transitions; no lane-side tracker mutation was attempted.

Ledger: ran `ledger-mark.sh sharingboundary-ported.txt tests/t2_sharing_boundary.rs` from this worktree (1 newly ported, 11 newly partial), then `./target/debug/am-dev obligations mark --status ported --ids-file sharingboundary-ported-ids.txt` to explicitly mark all three fully exercised methods ported, including previously accounted methods. Three fully ported methods: unsafe duplicate/ambiguous configuration; symlink/alias/two-root refusal; custom-bank self-authentication refusal. All other annotated methods remain partial, including the frontier-gated obligations and two navigation/discovery integration obligations. Ledger inputs are retained beside this report.

## Dependency-gated tests

Main explicitly approved these ten complete test bodies remaining ignored until recovery_frontier lands:
`canonical_deletion_needs_prior_ownership_without_changing_index`,
`changed_configuration_invalidates_prepared_write_without_mutation`,
`configured_branch_and_remote_default_never_use_feature_branch`,
`default_boundary_tracks_only_markdown_without_awa_vox_or_writes`,
`ignored_untracked_and_tracked_artifacts_block_eligibility`,
`lost_local_setting_discovers_nested_bank_for_boundary`,
`nested_boundary_and_observed_write_keep_exact_prefix`,
`selected_secret_remote_is_withheld`,
`unmerged_and_nonordinary_index_entries_block_readiness`,
`unsafe_or_interrupted_bank_never_returns_eligible_empty_success`.

Initial debugging runs correctly refused with MissingProvider rather than any fake frontier. Early test-only issues (private digest access, repeated provider registration, frontmatter quoting expectation) were corrected; no production behavior was weakened.

## Byte-exact dependency refresh

All 56 integration changes between a725b97 and b6ae1e34 were refreshed with Main's `git show b6ae1e34:<path> > <path>` recipe, then the three owned module/helper/table appends were restored. No source was copied from another lane's uncommitted snapshot. Security vector verification: `shasum -a 256 tests/vectors/security-regex-v1.json` = `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41`.

Exact refreshed paths (brace lists expand individual filenames):
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/{diaginit,layoutcore,migration,ompboundary,privateseam,sharingrecovery}-report.md`
- `src/{bank_state,bank_writes,claude_turns,diagnostics,error,host,initialization,installation,layout_recovery_context,layout_recovery_temps,layout_resolution,layout_restore,layout_units,lib,migration,omp_boundary,private_state,recovery_quiescence,sharing_recovery,turn_check,uninstallation,updating}.rs`
- `src/cli/helpers/{diagnostics,initialization,migration,mod,omp_boundary,sharing_recovery}.rs`
- `src/cli/{mod,table,wiring}.rs`
- `src/fs/descriptor.rs`
- `src/installation/{plan,seams,transaction}.rs`
- `src/migration/{command,plan,recovery}.rs`
- `tests/obligations/python-baseline-v1.json`
- `tests/{t1_private_state,t2_diagnostics,t2_initialization,t2_installation,t2_layout_core,t2_layout_core_probes,t2_lifecycle,t2_migration,t2_omp_boundary,t2_sharing_recovery}.rs`
- `tests/wiring-pending.txt`.

## Final whole-suite result

`cargo test --offline --no-fail-fast --features dev-tools` completed successfully on macOS arm64 after ledger marking: tool result reported **1018 passed, 67 suites, 94 ignored, 1817 filtered**; wall time 561.72 s. Retained full output: `artifact://12827`. No non-owned fix or test suppression was needed beyond Main-approved dependency gates in this lane. Implementation, CLI composition, ledger and all currently reachable finish-line checks are complete.
