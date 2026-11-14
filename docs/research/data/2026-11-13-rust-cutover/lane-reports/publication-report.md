# publication lane report

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-publication`. Base: a725b97. Non-owned files were refreshed byte-exact from `d1b2766` and then from `b6ae1e34` (the commit that brought in sharing_recovery) using `git -C /opt/agent-memory-worktrees/rust-v015-int show <hash>:<path> > <path>`. All 56 A/M paths between a725b97 and b6ae1e34 were refreshed this way. `tests/vectors/security-regex-v1.json` has sha256 `26c762b329c66b51…`. No Git writes, no Python execution, no sub-agents, no new crates.

## Files

Created (owned):
- `src/memory_publication.rs`: port of `memory_publication.py`.
- `src/layout_recovery_publication.rs`: port of `layout_recovery_publication.py`.
- `src/cli/helpers/memory_publication.rs`: the `am publish` helper (`publish_memory.py`).
- `tests/t2_publication.rs`.

Shared files, append-only changes (each one is a separate, unchanged diff against b6ae1e34):
- `src/lib.rs`: added `pub mod layout_recovery_publication;` and `pub mod memory_publication;` in their alphabetical positions.
- `src/cli/helpers/mod.rs`: added `pub mod memory_publication;`.
- `src/cli/table.rs`: added `route("publish", &helpers::memory_publication::HELPER)` at its baseline MEMORY_ROUTES position (after `private`, before `exchange`).
- `src/cli/mod.rs` `register_providers`: appended `turn_sources::register_publication_gate(Some(Arc::new(memory_publication::Gate)))`.
- `tests/wiring-pending.txt`: removed `turn_sources::register_publication_gate waits for memory_publication, sharing_recovery`.
- `src/bank_writes.rs` `RecoveryFrontier`: appended the required method `require_complete(repo, owner, transaction) -> Result<Value>` (baseline `recovery_frontier.require_complete`). The future implementer is recovery_frontier.
- `src/sharing_hold_mapping.rs` `SemanticSharing`: appended three required methods, `check(repo, proof, owned_lock)`, `final_sources(root, observation, binding, owned_lock)` and `read_input(path, version2)`. Also added `pub(crate) fn semantic_sharing_provider()` after `register_semantic_sharing`; when nothing is registered it returns `Kind::MissingProvider`. The future implementer is semantic_sharing. The pending line `sharing_hold_mapping::register_semantic_sharing waits for semantic_sharing` already existed.

No foundation changes.

## Public API

`memory_publication`:
- Constants and environment: `SCHEMA`, `FIELDS`, `LIMIT`, `API_VERSION`, `TIMEOUT` (30 s), `MAX_INDEX_BYTES`, `environment()`.
- Transport: `command(argv, env, payload)` and `git_write(root, args, GitWrite{payload,index,network,optional})`.
- GitHub API: `api(endpoint, method, body, allow_missing)`, `collection`, `repository`, `restricted_push_actor`, `policy`.
- Results and index: `result`, `index_bytes` (sha256 or null).
- Candidate and request: `candidate`, `prepare(repo, proof)`, `request_value(root, value) -> binding`, `publication_sources`.
- Pull requests and checks: `publication_pr`, `check_inventory`, `required_checks_pass`, `protected_candidate`.
- Landing and CLI: `land(repo, request)`, `command_result(operation, repo, input, output, v2, pack)` (the baseline `main` envelope).
- `Gate`, which implements `turn_sources::PublicationGate`:
  - `request_value` runs the full baseline validation.
  - `replay` forwards to `sharing_recovery::replay(repo, request, quarantine_records)`.

`layout_recovery_publication`:
- Constants: `MAX_*`, `OBSERVATION`, `REVIEW`, `REQUEST`, `TERMINAL`.
- Helpers used by retention: `require`, `exact`, `text`, `result`.
- `operation_evidence(repo, op, owner, runtime_root) -> Evidence{root,bank,base,directory,terminal}`.
- `observe_tree(repo, revision, prefix, directory, expected, runtime_root)`.
- Commands: `inspect(repo, op, input, runtime_root)`, `prepare(repo, op, input, runtime_root)`, `land(repo, op, input, runtime_root)`.
- `RecoveryPublicationContext`: an internal typed source guard that is never deserialized. `memory_publication::publication_sources/publication_pr/protected_candidate` take `Option<&RecoveryPublicationContext>`, so the baseline `type(...) is` check is enforced by the type system.

`runtime_root` is explicit, following `layout_recovery_state`. The future caller is the layout_recovery `recover-layout publish …` handler.

## CLI

`am publish` is the port of `publish_memory.py` (module `memory_publication`). Its parser:
- positional `operation` with choices `{prepare,land}`;
- `--repo` (required);
- `--input` (required);
- `--output`;
- `--v2`, a store_true flag with the baseline help text.

The description is the module docstring. Success prints indent-2 ASCII JSON and exits 0. Every refusal prints the baseline compact `publication-result-v1` envelope and exits 1:
- `pending-inspection` with capacity details;
- `pending-inspection` with `known-dispute-state` and the failure code and repair;
- `pending-private-output` / `private-output-invalid`;
- `pending-inspection` / `publication-unverified`.

## Seams

No new seam and no new pending line. sharing_recovery is called directly (Main's decision). semantic_sharing and recovery_frontier are reached through the existing traits above, so the typed refusal is MissingProvider until they are registered. In this runtime, `prepare` and `land` therefore refuse until semantic_sharing and recovery_frontier land; the CLI returns `pending-inspection`. The PublicationGate is registered and active: request validation and replay use real code. Replay itself still needs sharing_boundary, which sharing_recovery waits for.

## Tests (`tests/t2_publication.rs`)

The offline GitHub protocol fixture (`publication_github.py` SERVER and GUARD) is ported to Rust. `gh` and the bare remote's `pre-receive` are POSIX shims that re-execute this test binary (`github_fixture_worker` and `github_guard_worker`). State is a JSON file guarded by flock, and the Git transport rewrite is a sh wrapper. The library is driven in a child (`policy_worker`) that has the World environment, so no process-global environment is touched.

Active tests (14), all green:
- Policy:
  - rule-page pagination and unsupported rules;
  - protected review, checks, App ID and permission/unavailable refusals;
  - restricted push actor: all 12 negative identity/inventory cases;
  - complete later page vs. broken, failed, wrong-status, over-bound and duplicate chains.
- Protected landing (`protected_candidate`): review pending → checks pending → missing check → exact generated merge. The test checks parents `[base, candidate]`, an equal tree, and that no `refs/agent-memory/*` is left over.
- `Gate`: refuses an inexact request; replay is forwarded (same kind and message as calling `sharing_recovery::replay` directly).
- `request_value` refusals for shape, schema, repo, project, base and repository.
- `am publish` with no semantic provider: exact refusal envelope, and the bank, index and HEAD are unchanged.
- `am publish` parser: exit code 2.
- Layout `exact`, `text` and `result`.
- `observe_tree` against real Git: every blob's identity (SHA-1 framing), sha256/size, retained bytes, stable source IDs on re-observation, and refusal of a changed inventory.
- `observe_tree` refuses an executable object and a committed runtime marker.

Obligations are annotated and ignored, so all are partial; none is fully ported.
- 19 memory publication obligations: test_memory_publication ×4, test_publication_policy ×2, test_publication_restricted_actor ×5, test_publication_pending ×5, test_publication_recovery ×2, test_publication_worktree ×1.
  - These have full Rust bodies against the ported fixture.
  - They are `#[ignore = "needs semantic_sharing, sharing_boundary, sharing_recovery registered"]`, because the proof comes from `am semantic inspect/review` and identity binding needs recovery_frontier.
  - The bodies have not been executed.
- test_publication_admission ×1 and test_publication_hold_recovery ×1: ignored; their bodies call `deferred_body`. They need the AdmissionContract and QuarantinedSharingVersionTwoContract scenario fixtures, which belong to the admission and semantic_sharing lanes.
- 22 layout publication obligations: test_layout_publication_faults ×4, test_layout_recovery_publication ×10, test_layout_recovery_publication_closure ×6, test_layout_recovery_resolved_publication ×1, test_layout_recovery_identity_closure (publication) ×1.
  - These are ignored with `needs layout_recovery, recovery_frontier, sharing_recovery registered`, and their bodies call `deferred_body`.
  - They drive `am bank recover-layout publish …` over `layout_recovery_fixtures.public_repair`. That fixture and command are unported and belong to the layout_recovery lane, which ports these bodies with the fixture.

Ledger: `ledger-mark.sh - tests/t2_publication.rs` → see Verification.

## Declared deltas

- Child stdin: the baseline `subprocess.run(input=None)` inherited stdin. Rust gives transport children `/dev/null` (or the exact payload), so `git`/`gh` never consume the caller's stdin.
- Bounded readers:
  - Transport stdout is captured with a 1 MiB bound; `CaptureLimit` maps to the baseline "exceeds its complete bounded reader" refusal.
  - The raw `cat-file --batch` output goes through a bounded in-memory capture (limit = Σsizes + 128·n, RLIMIT_FSIZE pinned to limit+1) instead of a temp-file spool, as sharing_recovery also declared. All framing and identity checks complete before any blob is retained. The baseline generator could have retained earlier blobs before a later framing failure.
  - The original index is hashed by streaming with a 512 MiB bound and must be a regular, no-follow file. The baseline read it whole and unbounded.
- Runtime artifact digest: `runtime_artifact_digest` is the design's one artifact digest (pack inventory plus the executing `bin/am`, via `artifact::runtime_digest`). The baseline digest was pack-only. A Python-era observation therefore never matches, so the request refuses as "runtime changed".
- Regex: the regex crate here has no Unicode-perl classes, so `\b`/`\s` in the status and Link patterns are ASCII.
- Time: check recency uses the RFC3339 parser `lifecycle::time_value` rather than `datetime.fromisoformat`.
- Type and decode failures:
  - Python TypeError/AttributeError/KeyError and UnicodeDecodeError on malformed GitHub, Git or JSON values become ContractErrors with the nearest baseline refusal text. Under `main` the output is identical (`pending-inspection`).
  - Inside `restricted_push_actor`, every failure (including non-UTF-8 `gh` output) becomes `eligible: false`, where Python would have crashed to `pending-inspection`.
  - A non-list `contexts`/`checks`/`required_reviewers` in a rule refuses; Python would have iterated a str or dict.
- `land`'s semantic-check `except ContractError → pending-reconciliation` also propagates `MissingProvider`, as well as HoldFailure and capacity, so a missing provider is never reported as a source change.
- `--output` when the runtime pack is not registered (non-installed dev binary): the result is `pending-private-output` with the pack_root error text. The installed runtime always has the pack.

## Commands (aarch64-apple-darwin, Darwin 27.3.0 arm64, rustc 1.99.1 58cf8f9c3)

See Verification.

## Verification
Platform: aarch64-apple-darwin, Darwin 27.3.0 arm64, rustc 1.99.1 (58cf8f9c3 2026-10-13). No Linux run claimed.

- `cargo fmt --check`: clean.
- `cargo check --offline --all-targets --features dev-tools`: 0 warnings, 0 errors.
- `cargo test --offline --no-fail-fast --features dev-tools` (full suite, 570 s): every target green except `obligations_ledger::live_ledger_is_consistent` ("43 findings" = this lane's not-yet-marked annotations), run before marking.
- `~/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh - tests/t2_publication.rs`: marked ported=0 partial=43 (ledger summary partial 374 / ported 282 / unported 1206).
- After marking: `cargo test … --test obligations_ledger --test wiring --test t2_cli`: 5/24/2 passed, 0 failed.
- `cargo test --offline --features dev-tools --test t2_publication` ×3: 15 passed, 0 failed, 43 ignored each time (≈13.5 s).
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/memory_publication.rs src/layout_recovery_publication.rs src/cli/helpers/memory_publication.rs`: only the six `Regex::new(<literal>).expect(...)` statics in memory_publication (invariant: valid literal patterns, commented). No slicing/indexing on input; no `unsafe` (the test file's flock uses libc in tests only).

## Not ported / partial (exact)

- All 43 annotated obligations are partial (ignored). Bodies exist for 19 memory publication tests but have not been executed. 24 bodies are deferred to their owning lanes' fixtures (listed above).
- Every baseline function of both modules is ported; there is no omitted branch. At runtime, `memory_publication.prepare/land` and `layout_recovery_publication.inspect/prepare/land` refuse MissingProvider until semantic_sharing (`check`, `final_sources`, `read_input`) and recovery_frontier (`require_complete`, `observe_owned`, `ledger`, `evidence`, `check`) are registered.
