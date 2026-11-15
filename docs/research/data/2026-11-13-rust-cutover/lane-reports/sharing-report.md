# Sharing lane

Base: detached cc85cc37, refreshed byte-exact with Main's approved c8c7feb5 and then 5bd11582. No Git or GitHub writes; no Python execution, subagents, or new crates. Required sharing_frontier checkpoint was sent after its first five contracts passed.

## Files and implementation

New: `src/sharing_frontier.rs`, `src/semantic_sharing.rs`, `src/sharing_page_export.rs`, `src/cli/helpers/semantic_sharing.rs`, `tests/t2_sharing.rs`.

Lane changes to existing files: `src/lib.rs`, `src/error.rs`, `src/sharing_hold_mapping.rs`, `src/cli/mod.rs`, `src/cli/table.rs`, `src/cli/helpers/mod.rs`, `src/consolidation.rs`, `tests/wiring-pending.txt`, `tests/obligations/python-baseline-v1.json`. Other files were refreshed only byte-exact from Main's supplied integration hashes; they contain no lane edits.

All baseline runtime functions in sharing_frontier (including changed-component identity, bounded selectors, known holds, withholding closure, disclosure pages and source-bound review preparation), semantic_sharing (v1/v2 inspection, review, consolidation, dependent masking, invalidation preservation, proof checking, indexed review and readers), and sharing_page_export (exclusive output preflight, descriptor-bound readback, final source/hold/frontier checks and last completion marker) are implemented. No runtime function is deferred.

Public APIs retain baseline names. `semantic_sharing::Snapshot` is `(PathBuf, Value, Value, Value, Value)`; owned-lock arguments are explicit. `sharing_frontier::{request_value,inspect}` are public for memory_refresh. The existing `sharing_hold_mapping::SemanticSharing` is implemented by `semantic_sharing::SemanticSharing`, including check/final_sources/read_input and Main's requested inspect/snapshots additions. The composition root registers it once; its pending line is removed. Existing bank_writes::RecoveryFrontier is the only frontier seam. No second provider seam was added.

`am semantic` dispatches reconcile_memory.py's inspect/review/check, inspect-v2/page-v2/worklist-v2/review-v2/review-indexed-v2/recover-holds commands with all baseline flags. The route is before sharing, as required. `consolidation::AXES` is made pub(crate), with Main's approval; child_id/build were already public and needed no visibility change.

Error addition: typed `Kind::PrivateOutput` plus its constructor, so explicit output refusals retain output/private-output-invalid without exposing ordinary semantic source failures. No unsafe or foundation primitive was added.

## Contracts and remaining verification

Ledger helper run from this worktree: `ledger-mark.sh <sharing-ported-names> tests/t2_sharing.rs`; it marked 5 ported and 19 partial obligations. The temporary names file was removed.

Fully ported: all five SelectorLocalReuseContract methods. The bounded-reuse test crosses the real 24 MiB rendered-cache threshold, instead of monkeypatching Python cache constants.

All nine SemanticSharingContract, five SemanticIntroductionsContract and five CheckedPageExportContract methods have annotated Rust coverage, conservatively partial:

- Seven real-clone tests are gated exactly `needs recovery_frontier registered`: cross-file conflict, explicit dependencies, duplicate consolidation, hidden superseder, clarified-correction consumer, empty export, and complete bundle/public-page equivalence. Main confirmed recovery_frontier is not yet integrated. Tests use a child-process source-PackRoot adapter pointing to the real worktree pack, following the established initialization pattern; no domain provider is mocked.
- Direct semantic tests cover nonoverlapping prose and overlap refusal, all-side disjointness/unknown conditions, complete pair decisions and malformed proofs, and a newly held context's preserved omission. Their remaining baseline clauses are fresh authority observations, source mutation/tampered-output refusal after successful review, canonical transaction application and actual recall.
- Direct introduction tests cover router introductions, compatible introduction merge, route-purpose masking, revoked invalidation retention and multiple source-scope variants. Remaining baseline clauses: custom-index introduction/move cases, read-when/omission/new-index-route variants, terminal erasure/tombstone and projection-invalidity checks, real exchange/application and feature-state preservation.
- Direct export tests cover complete nontruncated multi-page payloads, collision/unsafe-parent/symlink/relative/same-path refusals, descriptor readback corruption and link refusal, without a completion marker. Remaining clauses: genuine source records across exported pages, nested output overlap, source/hold/frontier mutation, injected write/fsync/readback failures and FIFO substitution. The gated complete-bundle tests additionally verify hashes, permissions, marker fields, public pages and unchanged canonical/feature state when their provider becomes available.
- The clarified-correction fixture starts after governance's correction and tests sharing's consumer phase, not governance's admission/explicit-authority workflow. Quarantined-sharing plus memory_refresh/imported-recall and full known-hold/publication workflows still require their integrated cross-lane contracts; this report does not claim those campaigns ran.

Additional non-ledger tests cover observation-time-insensitive versus inherited-framing-sensitive identities, terminal identity relations, bounded/schema-dispatched input and private refusal text.

## Declared deltas

Cache charging follows owned UTF-8 Rust storage rather than CPython sys.getsizeof; an uncached source/query is still fully matched, with no truncation. Python's interpreter-version-dependent repr of nonprinting Unicode in effective framing is not reproduced exactly: the stable selector rendering retains literal Unicode except standard backslash/quote/control escapes. Existing strict codec deltas remain in force. Reads are explicitly bounded, and subprocess observations use the existing finite-deadline Git foundation.

## Evidence

Platform: aarch64-apple-darwin, Darwin 27.3.0 / xnu-13432.60.57~99, rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1. Linux was not run.

- `cargo fmt --check`: clean after the final integration refresh.
- `cargo check --offline --all-targets --features dev-tools`: clean, zero warnings.
- Latest completed target before the final acceptance batch: 22 passed, 7 ignored.
- Final acceptance batch: `cargo test --offline --features dev-tools --test t2_sharing` passed three consecutive times: 22 passed, 7 ignored each (8.98s, 8.98s, 8.94s). `cargo test --offline --no-fail-fast --features dev-tools` passed: reported 1,204 passed across 78 suites, 244 ignored and 1,817 filtered. Batch completed successfully in 641.67s; evidence `artifact://13674`.
- Required source-input grep `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/sharing_frontier.rs src/semantic_sharing.rs src/sharing_page_export.rs`: no matches.
- Security vector sha256: `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41`.

AWA status was read: managed repository robertelee78/agent-memory. Main owns all GitHub/accountability writes and integration; no independent lane phase or delivery claim was written.
