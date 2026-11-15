# Delivery review fixes

## Scope and source

Assignment base: `06c186da42e356c211d19f42fe6057f2da96b92e`, worktree `/opt/agent-memory-rust-delivery-fix`, branch `lane/delivery-fix` (assignment-provided identities; no Git validation run). The independent source report is `/opt/agent-memory-rust-reviews/docs/research/data/2026-11-13-rust-cutover/lane-reports/delivery-review.md`, reviewing `2457005f8447d32bd1f4958654001f323c29ea34`. ADR-0029, the complete cutover design, lane brief and original delivery report were read. Baseline source was inspected as specification only; its reported failures were not rerun.

Changed files only:

- `src/release_distribution.rs`
- `src/bin/am_dev/release.rs`
- `tests/t2_delivery.rs`
- This required report.

No cache-recovery, ledger, integration-branch or other-worktree edits; no commit or push.

## Findings and fixes

1. **F1 / P1 — ordered command surface after publication.** The builder signed recursively sorted bytes but returned its original insertion-ordered `Value`; public readback reparsed sorted objects, so ordinary `Value` equality refused after publication. The builder now returns the already verified/reparsed manifest. Signed encoding remains the existing `ReleaseManifest::encoded()`; global JSON equality is unchanged. A nested unsorted object/array surface is built, cryptographically verified, published, publicly acquired, promoted and read back; exact published resume must not append any service mutation. The same regression also exercises recovery after an interrupted upload leaving one draft asset.

2. **F2 / P2 — bounding full rather than consumed gh responses.** `MAX_GH_RESPONSE` remains an explicit 65,536-byte finite capture bound, with existing parser bounds and command deadlines. API calls now project consumed fields before capture using gh `--jq`: comparison `.status`; release tag/draft/prerelease/target/html URL and asset name/size; branch protection/commit SHA; tag object type/SHA; tree completeness and row path/type/mode/SHA; branch-rule array emptiness. The tag lookup retains its 60-second deadline with the added projection arguments. The simulator actually removes oversized note bodies and extracts comparison status from oversized recorded responses; the publication regression demonstrates complete publication and idempotent resume despite those raw payloads being refused by the capture boundary. Separate excessive projected status and release URL fixtures must refuse before publication/default activation (the latter preserves the created draft without uploads).

3. **F3 / P3 — public diagnostic insertion order.** Download error details now insert `attempts` before `attempt_count`, without copying the attempts vector. The real terminal-404 transport fixture is encoded with the insertion-order public CLI codec and checked against literal byte fragments covering the diagnostic prefix, complete stable attempt fields and the array-to-count transition. Nondeterministic timing/path values are not golden. The regression also asserts one attempt and permanent HTTP refusal.

4. **F4 / P2 — relative signing-key path.** Tilde-expanded paths are anchored to the actual process working directory and normalized lexically before foundation trust inspection. No symlink resolution precedes that inspection. The complete builder/publication fixture uses a real disposable key via `../key`; a relative final symlink refuses before output, the same valid fixture key refuses against the production signer, and an owner-nonprivate key retains the custody refusal.

5. **F5 / P2 — metadata declaration regex parity.** Package identity splits on literal LF, matches only the exact `metadata: ` prefix followed by braces with a nonempty body, and does not strip CR. Consumer package-identity regressions cover prose metadata, an ignored empty-brace line alongside a valid declaration, a final line without LF, CRLF refusal, empty braces, nonempty braces lacking a version, trailing whitespace, duplicate declarations, and declaration-count precedence over malformed JSON.

## Authority, API and fixture boundaries

Public API signatures, CLI arguments, providers and production trust defaults are unchanged. Private `build_trusted` / `publish_trusted` workers thread explicit signer authority; public production `build` and `publish` always pass compiled `ALLOWED_SIGNERS`. All signature creation and namespaced verification use real `ssh-keygen`; no SSH executable is replaced and no public key is misrepresented. These private workers and inline unit fixtures were explicitly approved by Main. No generic signer registry, environment trust override, new crate or production fake fallback was added.

The fixture's disposable signer authority is honest and test-local. It does **not** qualify access to, custody of, or publication with the real production private key. Portable fixture contents retain the real pinned allowed-signers file; signing authority is supplied only to the private worker under test.

Publication unit tests reexec only their exact test in supervised finite-deadline children. Each child receives a fresh minimal environment, isolated HOME/TMPDIR/Git configuration, fixed Git author/committer timestamps and a private canonicalized root. They do not mutate process-global HOME/PATH/environment. Only gh, remote Git operations and curl are simulated through private PATH executables. Local Git object reads and SSH operations are real; unknown remote/service operations refuse, and no service command can reach credentials or the network. Recorded identities are loaded from fixture files, not injected through global environment mutation. Test binaries/candidate assets are never executed.

## Exact new tests

Library unit tests (`--features dev-tools --lib`, module `release_builder::tests`):

- `release_builder::tests::publication_completion_and_resume_canonicalize_nested_surface_and_project_large_responses`
- `release_builder::tests::publication_refuses_excessive_projected_compare_and_release_before_activation`
- `release_builder::tests::relative_signing_key_is_anchored_lexically_and_retains_custody_refusals`

Integration tests (`--features dev-tools --test t2_delivery`):

- `portable_metadata_matches_literal_nonempty_lf_declarations_without_normalizing_cr`
- `terminal_download_failure_preserves_baseline_cli_details_insertion_order`

No requested finding or requested regression branch is intentionally deferred. Broader production release qualification and legacy integration obligations from the original lane report are not newly claimed complete by these fixtures.

## Verification and handoff

No Python, Cargo build/check/test, smoke run, linter or formatter was executed. No live API, publication, credential access, Git-write command, commit or push was executed by this worker. Rust LSP was reported unavailable in the assignment; source work used textual inspection. No foundation files changed. Runtime panic-capable operations added here are confined to `#[cfg(test)]` fixtures; production path anchoring and projection code add no unchecked indexing/slicing or unwrap/expect.

Parent verification completed on Ubuntu 26.04, Linux 7.0.11 x86_64, rustc 1.94.1:

| Check | GNU | x86_64 musl |
| --- | --- | --- |
| `cargo fmt --check` | Passed after edit-only reflows | Same source |
| `cargo check --offline --locked --all-targets --features dev-tools` | Passed, zero warnings | All targets compiled in suite |
| `cargo test --offline --locked --features dev-tools --lib release_builder::tests` | 3 passed, three consecutive runs | 3 passed, three consecutive runs |
| `cargo test --offline --locked --features dev-tools --test t2_delivery` | 33 passed, three consecutive runs | 33 passed, three consecutive runs |
| Full suite `--no-fail-fast --features dev-tools` | 476 passed, 0 failed, 2 existing dependency ignores | 476 passed, 0 failed, 2 existing dependency ignores |

Musl commands add `--target x86_64-unknown-linux-musl` and use an isolated Cargo target directory. No new test was ignored. Parent retained the initial simulator failures and corrected only their fixture routing before these consecutive passing runs; see below.

Independent reviewer-model `AcceptDeliveryFix` (`anthropic/claude-opus-5-5:high`), separate from the fixer, accepted all five corrections after source inspection. Production public entrypoints still use compiled signer authority; actual disposable SSH signatures and private-worker publication/resume are exercised without live publication. The reviewer did not run tests or gh/gojq: tree/branch/tag projection syntax was source-checked, not independently exercised by a real networked gh invocation. This proof does not qualify production key custody, actual release artifacts or live GitHub publication.

The required panic-site scan is retained in the proof archive. Added unwrap/expect operations are inside the isolated `#[cfg(test)]` fixtures; pre-existing archive slices retain their checked bounds. No new input-derived panic site was identified in the production fix.

Evidence: `/tmp/agent-memory-delivery-fix-proof/` on `omp-beast`, including initial failures, corrected focused runs and both full suites. Parent owns tracker/Vox handoff, commit/push and integration; no macOS/aarch64 execution is claimed.

## Parent proof and fixture-routing correction

Main reported a clean zero-warning all-target dev-tools check, then the first GNU and musl library proofs failed two publication tests at the real main-promotion policy check; the relative-key test passed. Failed logs remain at `/tmp/agent-memory-delivery-fix-proof/gnu-lib-1.log` and `/tmp/agent-memory-delivery-fix-proof/musl-lib-1.log`. No worker rerun was performed.

Source diagnosis: the test gh case pattern `*/branches/main` also matched `repos/robertelee78/agent-memory/rules/branches/main`, shadowing the recorded rules/projection handler and returning a branch object instead of the projected boolean. The fixture branch route now matches the exact recorded branch endpoint, allowing the distinct rules handler to receive its own request. This is a one-line fixture-routing correction; production projection, protection checks and refusal behavior are unchanged. Formatting-only edits requested after Main's formatter check were also applied through `edit`; no file-mutating formatter ran. Parent then observed the passing focused and full runs above.
