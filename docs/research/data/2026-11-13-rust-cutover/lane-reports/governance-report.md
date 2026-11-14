# Governance lane report (v0.0.15 Rust cutover)

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-governance`, detached from `8f69d726`, refreshed byte-exact (`git show <hash>:<path> > <path>`, every non-owned file re-compared with `cmp`) to `dc908552` (checkpoint + inactive), `fba4152d` (retrieval, discovery, projections) and finally `c8c7feb5` (comparison, consolidation, publication, OMP transport) on Main's instruction; lane appends re-applied after each refresh. `tests/vectors/security-regex-v1.json` sha256 = `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41`. No Git writes, no Python executed, no sub-agents, no new crates, no `unsafe`.

STATUS: complete on `c8c7feb5`; finish-line results at the end. Governance calls `comparison`, `retrieval`, `inactive` directly (Main's instruction); the scratch stand-in used while `comparison` was pending was deleted and never entered the lane tree.

## Files

Created (owned):
- `src/governance.rs` — port of `governance.py` (670 lines).
- `src/cli/helpers/governance.rs` — `govern_memory.py` helper (parser + typed handler).
- `tests/t2_governance.rs` — contract tests.

Shared, append-only (re-applied after each refresh):
- `src/lib.rs`: `pub mod governance;` (alphabetical, after `fs`).
- `src/cli/helpers/mod.rs`: `pub mod governance;`.
- `src/cli/table.rs`: routes `govern`, `correct`, `supersede`, `revoke`, `forget` → `helpers::governance::HELPER`, inserted after `checkpoint`, before `publish` (baseline `MEMORY_ROUTES` order).
- `src/cli/mod.rs` `register_providers`: `register_transaction_validator(op, Governance)` for `correct`, `supersede`, `revoke`, `forget` (the baseline `WRITE_ACTION` values), appended at the end.

Approved non-append edit (Main): `src/cli/helpers/admission.rs` `fn private_preview` → `pub(crate) fn private_preview` (baseline `recovery_frontier.private_preview`, single definition reused by the governance helper).

Not touched: `tests/wiring-pending.txt` (no line waits for governance; governance introduces no registration point). `tests/obligations/python-baseline-v1.json`: 28 governance obligations marked `partial` by `ledger-mark.sh`.

Test-only: `#[allow(dead_code)]` on the `#[path = "support/admission.rs"] mod support;` include (its bound `fixture`/`state` helpers are used by `t2_admission` only).

## Public API (`agent_memory::governance`)

Constants `REQUEST_INPUT_LIMIT` (262144), `PROPOSAL_INPUT_LIMIT` (512 MiB), `FILE_LIMIT` (16384), `OPERATIONS`; `write_action`, `result`, `reference`, `union`, `target_value`, `request_value`, `Observation` (typed form of the baseline observation dict), `observe`, `pending`, `narrower_conditions`, `permitted_impact`, `copied_body`, `route`, `qualified_authority`, `Desired { Files, Deferred }`, `desired`, `normalize` (baseline `normalized_request`), `plan`, `preview`, `evidence_value`, `validate_transaction`, `check_sources`, `apply`, `require_intent` (the baseline `main(required_operations=…)` check), `Governance` (`bank_transactions::TransactionValidator`), `command_result`.

Every baseline function is ported: `result, reference, union, target_value, _request_value, request_value, _observe_current, _qualify_observation, observe, pending, narrower_conditions, permitted_impact, copied_body, review_deferral, next_version, archive, qualified_authority, reroute, route, delete_closure, desired, _normalized_request, normalized_request, _build_proposal, _proposal (inlined in preview), plan, preview, validate_transaction, check_sources, apply, main`. `desired` is split into one named helper per baseline branch (`delete_delta`, `deactivate_delta`, `requalify_delta`, `supersede_delta`, `history_event`); the supersession replacement event id reproduces `uuid.UUID(hex=sha256(op+':replacement-version')[:32], version=4)`.

Callees: `admission::{capture_value, check_sources, read_input}`, `comparison::{safe_text, request_value, inspect, judge, check}`, `retrieval::{request_value, observed_context, retrieve, git_context, scope_reasons}`, `inactive::compact`, `layout_resolution::{evidence_fields, claims_for, make_output_plan, output_plan_context, check_output_plan}`, `security::{repository_source, ensure_safe}`, `records`, `lifecycle`, `tombstones`, `bank_layout`, `bank_contract`, `bank_transactions`, `bank_writes`. `recovery_frontier` (unported) is reached only through the existing `bank_writes::RecoveryFrontier` methods `require_scope_owned`, `observe`, `check`; no method was added and no new seam was introduced.

## CLI

`am govern|correct|supersede|revoke|forget {plan --repo --request [--private-output] | preview --repo --request [--private-output] [--plan] [--operation-id] [--recorded-at] | apply --proposal}`. Inputs use the protected `admission::read_input` with the baseline bounds. The intent of the alias routes comes from `cli::intent` (`{'correct': ('correct',), 'supersede': ('supersede',), 'revoke': ('revoke',), 'forget': ('expire','retention','delete')}`) as `required_operations` and is checked after reading and before any observation, against the request (plan/preview) or `proposal_value(prepared).governance_evidence.request` (apply). Success: `INDENT2_UTF8` (indent=2, ensure_ascii=False), via `private_preview` for plan/preview. Failure: `DEFAULT` encoding, `verified:false`, `canonical_writes_performed` false (plan/preview) / null (apply), `recovery_may_be_required` (apply), the baseline generic error or the bank-capacity text plus `bank_capacity::details`.

## Contracts (`tests/t2_governance.rs`)

28 ledger obligations annotated (all ids verified present in the ledger): 21 `GovernanceContract`, 5 `GovernanceRecoveryContract`, `InstalledBankRoundtrip.test_governance_alias_checks_intent_at_preview_and_apply_before_writes`, `UnifiedCLIOperations.test_installed_correction_preserves_identity_origin_history_and_repeat`.

Executed now (17 tests): exact desired-state contracts on in-memory canonical banks built from the committed router/record templates, without the frontier — revocation compaction/event/route lifecycle; expiry boundary and policy retention; ambiguous/unconfirmed/assistant/narrower authority stays pending; current-context-only deletion; bounded derived deletion scope and content-free tombstones; copied ordinary-context and router assertions refuse deletion; empty nested index with custom context refuses, clean chain is pruned; existing replacement versioning, archive, two events (baseline UUIDv4 formula) and survival of a later deletion; stronger correction (version 2, `corrects`, origin-preserving archive); reactivation with an explicit current period; reuse of unchanged evidence refuses; narrowing vs scope correction; unresolved other-developer claim stays pending; request-shape refusals with baseline messages; CLI alias intent refusal at preview and apply with unchanged bank.

Ignored with full journeys implemented (28 tests, `needs recovery_frontier[, …] registered`): every baseline journey above through public `am` routes (`govern`/aliases, `capture`, `admission`, `compare`, `compact`, `projection`, `search`, `inactive-records`, `writes`, `transaction`, `bank`), including fault-injected interruption/rollback/receipt-corruption recovery with the `am` binary as an owned child. `journey_nested_completed_receipt_refuses_corruption_then_cleans_up` uses the bound fixture bank, not the baseline nested `docs/shared memory` sharing-boundary bank (that fixture is not ported); listed as a remaining gap of that obligation.

Ledger: all 28 are marked `partial` (no journey runs yet).

## Declared deltas

- Python `==` between JSON values treats `1 == 1.0 == True`; Rust compares typed values (and canonical encodings where the baseline compared `encoded`). Schemas make these fields strings/ints; no accepted input differs.
- `str.strip`/`str.split()`/`splitlines` use the shared Python-compatible helpers (`bank_contract::trim`, `pack_validation::splitlines`, the same whitespace class for `words`).
- Missing members that raised `KeyError` in the baseline are typed refusals (`Invalid governance evidence or canonical record`); the CLI envelope is unchanged (generic message).

## Foundation changes

None.

## Commands / platform

aarch64-apple-darwin, Darwin 27.3.0 arm64, rustc 1.99.1 (58cf8f9c3 2026-10-13). Linux targets not run.

## Finish line (macOS, `c8c7feb5` + lane)

- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: 0 warnings, 0 errors.
- `cargo test --offline --features dev-tools --test t2_governance` ×3 consecutive: each 17 passed, 0 failed, 28 ignored.
- `~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh - tests/t2_governance.rs`: `marked ported=0 partial=28` (ledger totals partial 501 / ported 298 / unported 1063).
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/governance.rs src/cli/helpers/governance.rs`: no matches.
- Full `cargo test --offline --no-fail-fast --features dev-tools`: green — 74 result lines, 1125 passed, 0 failed, 213 declared ignores; wall 613.65 s.

Remaining partial work per obligation: the 28 public journeys run once `recovery_frontier` (and, where named in each ignore reason, `recovery_transactions`) is registered; the nested sharing-boundary bank variant of the completed-receipt journey awaits that fixture.
