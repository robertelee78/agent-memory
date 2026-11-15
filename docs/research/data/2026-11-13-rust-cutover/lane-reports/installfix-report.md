# Installation independent-review follow-up

## Scope and platform

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-installfix`, detached integration baseline `fabc937`. No Git writes, Python execution, subagents, awa, Vox or GitHub. Platform: `aarch64-apple-darwin`, Darwin 27.3.0 (arm64), rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1. Other release targets were not executed here.

## Findings, changes and covering tests

| Review finding | Change | Covering tests |
| --- | --- | --- |
| F1 / P2 | The bootstrap manifest URL now uses `releases/download/v{CARGO_PKG_VERSION}/release-manifest.json`. Following assets use the manifest-selected, signature-verified version tag. | `bootstrap_manifest_and_following_assets_are_version_pinned`; release installation tests use only a pinned file:// tag directory, with no latest alias. |
| F2 / P3-2 | Added a local release-v2 publisher: throwaway ed25519 key, signed manifest, portable archive, and a stand-in bootstrap whose sha256/size are bound by every required signed target entry. The binary download asset is deliberately absent: a successful install therefore cannot have fetched a second binary. Fresh installs and runtime-root replacement cover copy and link modes, exact installed binary bytes, receipt bin/am digest, source release/target, exchanged old runtime stage and pre-activation backup. | `signed_bootstrap_installs_bound_bytes_and_exchanges_the_runtime_root`; `bootstrap_manifest_binding_refuses_version_digest_and_size_before_writes`; `staged_release_is_reverified_under_the_lifecycle_lock`. The latter corrupts binary, manifest and signature separately and calls prepare inside the lifecycle lock. |
| F2 vacuous adoption exclusion | Converted the pre-/post-commit adoption interruption regression to the signed release path. Removed the runtime-path exclusion; before commit the runtime must actually disappear, the old captured state must return, and after commit the installed binary must equal the bootstrap bytes. | `interrupted_adoption_rolls_back_before_commit_and_never_after`. |
| F3 / P3-3 | The four exhaustive install replay tests now install a signed release, not a development source. Both copy and link traces must contain payload-snapshot; source directory staging is interrupted too. Each selected before/after write is killed, recovered and retried. Comparisons now read actual node modes, links and bytes for every receipt target against the clean install (including the runtime source tree), rather than trusting receipt semantics alone. Recovery before commit also checks runtime/client/command removal and the exact original activation bytes; committed recovery compares physical target nodes. | `every_install_write_{before,after}_phase_interrupts_recovers_and_retries_in_{copy,link}_mode` (the existing 32 partition obligations). Exact residual baseline gaps are listed below; no claim that those gaps are fully covered. |
| F4 / P3-4 | derive.sh now emits genuine captured identities for files whose bytes did not change during relocation. Four identities.json fixture files preserve the original v0.0.14 activation identities. materialize asserts Rust's fingerprint and the captured receipt identity both match those independent identities, and leaves them intact. Only opaque trees and relocated bytes/links are rebound. | Every materialize-based adoption test, especially `captured_wrappers_and_hook_commands_are_exactly_the_recognised_python_era_bytes`; independent derive.sh reproduction in a scratch directory byte-compared all four fixture trees successfully. |
| F5 / P3-5 | Explicit --remote/--ref requests refuse with MANAGED_GUIDANCE before release acquisition or lifecycle writes, including Python-era receipts. A Rust-era development receipt skips an otherwise ignored bootstrap release stage. | `managed_channel_options_refuse_before_bootstrap_downloads` (remote, ref, both; fresh and captured installs; unusable manifest URL and full snapshot preservation); `repeated_development_bootstrap_does_not_stage_an_ignored_release`. |
| F6 / P3-6 | The per-action rollback compatibility gate now includes command-link alongside source/copy/link/wrapper. | `command_link_rollback_rechecks_bank_compatibility_before_restoring_python`: the initial compatibility check succeeds, the second simulates a concurrent incompatible bank migration and refuses before the Python wrapper is restored. |
| F7 / P3-7 | NOT OWNED: the am hook CLI route remains an integration/release-gate responsibility; no out-of-scope CLI or wiring-pending edits were made. | No new route-dispatch test in this lane. |
| F8 / P3-8 | console::bootstrap_command carries --bootstrap-binary PATH when rewriting a v3 receipt install to update, also with --check. | `bootstrap_rewrites_to_update_only_for_rust_era_receipts` now expects the preserved binary argument. Main was told the update parser/executor must accept and bind it. |
| F9 / P3-9 | verify_targets always performs copy/link artifact readback: runtime inspection for Rust-era sources, portable inspection for Python-era sources. | `python_development_artifact_readback_refuses_a_drifted_link_pack`: both complete and committed-activating Python journals first verify a healthy real pack, then refuse source drift without changing any on-disk state or promoting the journal. |
| Weak test: several in-flight Python steps | Added a genuine captured prior receipt with one journal containing source, copy, wrapper and activation mutations; recover restores every recorded physical identity and exact receipt bytes before adoption proceeds. | `in_flight_python_journal_restores_source_copy_wrapper_and_activation_before_adoption`; existing direct-adoption in-flight activation test remains. |
| Weak test: Python release source | Added a signed-bootstrap adoption case based on the genuine all-copy capture, with only channel metadata varied to release-kind. It preserves legacy source bytes. This is explicitly derived coverage, NOT a newly captured genuine Python release install. | `captured_python_state_with_release_source_is_adopted_by_the_signed_bootstrap`. Genuine capture limitation below. |
| Weak test: OpenCode near miss | Added exact Python-era OpenCode link recognition before adoption, matching the recorded old pack/hooks/opencode.js. Even a receipt rewritten to claim a near-miss link is refused as foreign, preserving state. | `adoption_refuses_an_opencode_hook_link_near_miss_even_when_receipted`. |

## Declared deltas and remaining coverage limits

- Operator-approved delta: managed --remote/--ref acquisition, managed-source copying into code, and staged Git-channel updating no longer exist. The historical managed-source-copy portions of the 32 install-write obligations are therefore replaced by signed release runtime-directory acquisition/staging. The existing managed source is retained as inert history; explicit channel flags refuse with re-bootstrap guidance.
- For those 32 obligations, baseline tests/lifecycle_fault_fixtures.py::LifecycleWriteCases.replay assertions not reproduced end-to-end are: creating a real bank via its baseline bank fixture and checking snapshot(memory) before/after every replay and idempotent repeat; calling the public doctor helper after each retry and asserting healthy['ok']; equality of the complete baseline semantics projection (the Rust test compares source fields except compatibility, mode/presets and target kinds/paths/identities/hook entries, but does not independently compare all baseline schema/custom_targets/enabled_adapters/bin_dir/data_roots/memory_data_location/runtime_state/uninstalled/artifact_digest/block_hash projection fields); exact equality of the entire interrupted trace event (the Rust replay asserts exit 86 and matching point, while clean trace checks step/phase/kind structure); baseline partition evidence emission via emit_partition. The baseline's installation_id-preservation branch is not exercised by these fresh-install replays (baseline_receipt is absent); separate capture-adoption tests cover installation_id preservation. Actual target-node comparisons and payload-snapshot in both modes are now covered.
- The replay exhausts every observed fresh release install logical write in each mode/phase, including source directory staging. It does not exhaustively fault every adoption or replacement write; adoption covers the two receipt commit boundaries plus multi-kind Python rollback, and runtime replacement separately covers successful directory exchange/backup.
- There is no genuine Python release-kind capture in the supplied capture corpus: its README and six scenarios cover managed/development installs, doctor and uninstall, and a search found no release-kind receipt. Executing Python to create one is prohibited. The derived release-kind test is labeled honestly. Main accepted this explicitly labeled derived case and confirmed genuine release-kind adoption moves to external release qualification: Main reports the published v0.0.14 one-line installer was attempted in disposable Worlds, but GitHub's unauthenticated API returned HTTP 403 (rate limit). A genuine signed v0.0.14 release adoption and old-updater refusal remain release-qualification requirements.
- F7 hook route and F8's update option/byte binding must land in their owning integration lanes. This lane only changes installation and its console rewrite.
- No obligation ledger was edited (outside ownership); the above limits must not be described as complete baseline doctor/bank/CLI integration proof.

## Files changed

- src/installation/seams.rs
- src/installation/transaction.rs
- src/installation/adoption.rs
- src/installation/plan.rs
- src/console.rs (bootstrap_command only)
- tests/t2_installation.rs
- tests/fixtures/v014-state/derive.sh
- tests/fixtures/v014-state/all-copy/identities.json (new)
- tests/fixtures/v014-state/all-link/identities.json (new)
- tests/fixtures/v014-state/claude-opencode-copy/identities.json (new)
- tests/fixtures/v014-state/codex-copy-pending-source/identities.json (new)

Public API signatures and registration traits are unchanged. Added only the internal pub(crate) require_hook_link recognizer. No foundation or dependency changes. Existing items were not renamed/removed/reordered; installation.rs was untouched to keep the LifecycleOps visibility/re-export changes independent. The upcoming OmpBoundary::native_transition signature change will also require adapting the existing FixtureOmp implementation in t2_installation.rs during integration; it retains the baseline signature here.

## Verification

- LSP status reported no language servers configured; no symbol rename/refactor was attempted.
- Initial target runs exposed two compile errors in new references/type comparison, then signed-fixture target-set, URL escaping, pinned-key and JSON object-order mistakes. Those were corrected; they are not reported as passing runs.
- cargo fmt --check: PASS after applying rustfmt's changed-line output through edit (not direct file writes).
- cargo check --offline --all-targets --features dev-tools: PASS, no warnings.
- grep -n 'unwrap()\|expect(\|\[.*\.\.' src/installation.rs src/installation/*.rs: no matches (grep status 1).
- Scratch-only derive.sh execution against the genuine external captures followed by diff -r of all four derived fixture trees: PASS. The scratch tree was removed; no Python ran.
- First final cargo test --offline --test t2_installation --no-fail-fast: PASS, 64 tests, 63.47s.
- Second and third consecutive final cargo test --offline --test t2_installation --no-fail-fast: PASS, 64 tests each (62.78s and 63.37s).
- Then cargo test --offline --no-fail-fast: PASS; runner summary: 636 passed, 49 suites, 28 ignored, 1,794 filtered. The combined two-target-runs-plus-whole-suite command exited 0 (425.75s).
- Final cargo fmt --check and cargo check --offline --all-targets --features dev-tools: PASS; final production grep again had no matches.

All assigned reachable fixes and tests are complete. Integration must merge the other lanes' owned interface changes and retain the explicitly accepted external qualification delta.
