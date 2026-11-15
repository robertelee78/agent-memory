# sharingrecovery lane report

## Scope and files

Ported the complete baseline `sharing_recovery.py` implementation into `src/sharing_recovery.rs`: hardened bounded Git commands; immutable commit IDs; bank/security eligibility with record-body quarantine; exact batched object metadata, framing and SHA-1/SHA-256 identity verification; complete authority locator/history inspection; disposable observation refs and cleanup; offline inspection; deletion-aware three-way structural reconciliation; and authority-mainline publication replay. Canonical bank files, index, branch, HEAD, remote-tracking refs and FETCH_HEAD are not mutated by these operations. Observation fetches and deletes only its UUID-named private ref.

Files created/modified in the lane:
- `src/sharing_recovery.rs`
- `tests/t2_sharing_recovery.rs`
- `src/cli/helpers/sharing_recovery.rs`
- `src/cli/helpers/mod.rs`
- `src/cli/table.rs`
- `src/lib.rs`
- `src/error.rs`
- `tests/wiring-pending.txt`
- `tests/obligations/python-baseline-v1.json`

External requested report: this file. External ledger name selection: `sharingrecovery-ported.txt` beside it. No Git/GitHub writes or Python execution. No crates were introduced by this lane: after Main's approved refresh to integration `a725b97`, object verification reuses its existing sha1/sha2 dependencies. No lane foundation changes. Main approved the CLI helper/declaration, exchange route, and alphabetical module placements.

Main subsequently authorized refreshing all 40 changed prerequisite files from `46a0100` to `a725b97`; these are imported integration content, not new lane work: `Cargo.lock`, `Cargo.toml`, `docs/research/data/2026-11-13-rust-cutover/lane-reports/{ompstream,recallcatalog,security}-report.md`, `src/cli/helpers/mod.rs`, `src/cli/helpers/security.rs`, `src/cli/mod.rs`, `src/cli/table.rs`, `src/error.rs`, `src/host.rs`, `src/lib.rs`, `src/model_reference.rs`, `src/omp_stream.rs`, `src/omp_stream/{clock_window,coverage,executor,frame,inventory,linux_image,run,wake}.rs`, `src/process.rs`, `src/recall_catalog.rs`, `src/security.rs`, `tests/obligations/python-baseline-v1.json`, `tests/t1_a1.rs`, `tests/t1_bank_transactions.rs`, `tests/t1_layout_history_workspace.rs`, `tests/t1_layout_recovery_state.rs`, `tests/t1_private_state.rs`, `tests/t1_project_identity.rs`, `tests/t1_scope_binding.rs`, `tests/t1_security.rs`, `tests/t2_layout_recovery_observation.rs`, `tests/t2_omp_stream.rs`, `tests/t2_recall_catalog.rs`, `tests/t_support_world.rs`, `tests/vectors/security-regex-v1.json`, and `tests/wiring-pending.txt`. The shared files retain this lane's additions; its ledger marks were reapplied locally.

## Public API and wiring

Primary operations: `inspect(repo, fetch)`, `observe(repo, owned_lock, quarantine_records)`, `reconcile(repo, base)`, and `replay(repo, request, quarantine_records)`. Shared pure/observation utilities: `raw_command`, `command`, `oid`, `commit`, `bounded`, `tree`, `server`, `authority_bank_location`, `first_bank_history`, `choose`, `mapping`, `header_order`, `merge_files`, `ancestor`, `result`, `command_result`, `fetch_failure`, `finish_observation`. Constants retain baseline bounds and authority repair text.

`SharingBoundary::inspect(repo, owned_lock) -> Result<Value>` is registered with `register_sharing_boundary`; the unregistered accessor returns `Kind::MissingProvider`. Its implementation belongs to `sharing_boundary`; Main will wire it at merge. Added its pending-wiring line. After the approved security refresh, this module calls `security::scan` directly; no security seam or duplicate scanner remains.

`turn_sources::PublicationGate` cannot be split without changing its required two-method interface. memory_publication must implement `request_value` with its real request/proof validation and implement `replay` by forwarding to public `sharing_recovery::replay(repo, request, quarantine_records)`. Then CLI `register_providers` registers that gate, and removes the existing `turn_sources::register_publication_gate waits for memory_publication, sharing_recovery` line. This lane deliberately does not install a half-validating gate.

Main's final contract correction says there is no SharingReplay seam: memory_publication and layout_recovery_publication call this module directly after integration. Required public signatures are present:
```rust
pub fn raw_command(repo: &Path, args: &[&str], network: bool, limit: usize, glob_pathspecs: bool, payload: Option<&[u8]>) -> Result<Vec<u8>>;
pub fn command(repo: &Path, args: &[&str], network: bool, limit: usize, glob_pathspecs: bool) -> Result<String>;
pub fn oid(value: &str) -> Result<&str>;
pub fn commit<'a>(repo: &Path, value: &'a str) -> Result<&'a str>;
pub fn tree(repo: &Path, revision: &str, prefix: &str, quarantine_records: bool) -> Result<Value>;
pub fn server(repo: &Path, remote: &str, branch: &str) -> Result<String>;
pub fn ancestor(repo: &Path, older: &str, newer: &str) -> Result<bool>;
pub fn authority_bank_location(repo: &Path, revision: &str, selected: Option<&str>) -> Result<()>;
pub fn first_bank_history(repo: &Path, head: &str) -> Result<()>;
pub fn replay(repo: &Path, request: &Value, quarantine_records: bool) -> Result<Value>;
```

Appended `Kind::SharingAuthority { code, component, repair }` and safe error constructors/message-preserving cleanup support. Authority advertisement, fetch and successful-observation cleanup retain their baseline failure codes and content-free repair guidance; a secondary cleanup failure preserves the primary error kind.

CLI: registered baseline `am exchange inspect --repo ... [--fetch]`, `reconcile --repo ... [--base ...]`, and `replay --repo ... --request ...`; request reads are capped at 65,536 bytes. Handler returns typed `CommandResult` with baseline pretty success and compact failure JSON, exit codes and uncertainty flags.

## Obligations and deferred integration

All 18 methods in `tests/contracts/test_sharing_recovery.py::SharingRecoveryContract` have real Rust contract bodies using isolated World environments, actual Git repositories/remotes, filesystem preservation comparisons, and public commands. They are marked `partial`, and ignored with named missing providers rather than fake implementations: all need sharing_boundary; offline diagnostics, fetch diagnostics and the three transaction/import tests additionally need memory_refresh. Real security is now present. No Rust implementation function/branch was intentionally omitted. These end-to-end bodies have not been executed while their remaining providers are absent.

`tests/contracts/test_observation_cleanup.py::ObservationCleanupContract.test_fetch_failure_survives_failed_cleanup` is ported and active: it verifies safe primary error attribution, code/component preservation, cleanup context and repair redaction through the extracted cleanup combinator. Six additional active integration-test entries cover immutable IDs, missing-vs-null reconciliation, metadata order, real bounded Git size/advertisement failures, byte-preserving real-bank independent merges, conflict refusal, project isolation and unsafe-content refusal (one entry is an isolated child worker). An active library unit test verifies SHA-1 Git blob identities against known empty/hello vectors.

Ledger helper, run from this lane after Main fixed its old hard-coded integration-root bug, marked 1 ported and 18 partial. The earlier helper invocation wrote centrally; Main explicitly reverted those marks and authorized the corrected lane-local invocation. No integration-tree edits were performed manually.

## Declared deltas

- Strict UTF-8 / BOM and surrogate/depth behavior follows the existing native codecs and the design's declared deltas, rather than Python's incidental decoding behavior.
- Git stdout is drained through bounded native process capture instead of a temporary-file spool; no remote output is allocated without a hard bound. Read-only Git children retain pinned RLIMIT_FSIZE, and all owned direct children retain fixed baseline 10s/30s deadlines. Stderr is directed to a held `/dev/null` descriptor, never copied into refusals.
- Missing runtime registrations remain typed MissingProvider refusals, including fetch inspection, rather than being converted into a plausible offline success.
- Non-authority primary errors retain their kind and safe cleanup text; Rust does not attach a Python-style dynamically mutable `repair` attribute to a generic ContractError. Authority failures carry repair explicitly in Kind.

## Verification

Platform: aarch64-apple-darwin, Darwin 27.3.0 arm64, rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1. No Linux run claimed.

Commands executed:
- `awa status --json`: managed repository, 164 managed issues; coordinator is the only GitHub writer, so no worker tracker reconciliation/write was attempted.
- `vox agent status --harness omp`: Ready, omp-work, room kh57aq2zwdss.
- `cargo fmt --check`: clean after formatting through anchored edits.
- `cargo check --offline --all-targets --features dev-tools`: clean, zero warnings.
- `cargo test --offline --features dev-tools --test t2_sharing_recovery`: final refreshed target passed three consecutive times, each 7 passed / 18 ignored. Earlier fixture failures (World override naming, invalid root-level source path, and treating Python dictionary equality as Rust object insertion-order equality) were corrected.
- `cargo test --offline --no-fail-fast --features dev-tools`: final corrected-byte run passed, exit 0: 906 passed across 60 suites, 67 ignored; wall time 491.88 seconds (`artifact://12648`). The preceding run's `nel-whitespace` failure was Unicode line-separator normalization in the refresh transport, not a security implementation defect. Restored the fixture with `git -C /opt/agent-memory-worktrees/rust-v015-int show a725b97:tests/vectors/security-regex-v1.json > tests/vectors/security-regex-v1.json`. Its SHA-256 is now exactly `26c762b329c66b51d5b064ec5996cd3cdcf5d8e91d5739c2357b86d614bc6f41` (95,608 bytes). Byte comparison checked all 40 refreshed paths: 34 match committed bytes exactly; the six shared files differ only by documented module/route/error/pending-wiring additions and this lane's 19 obligation marks plus totals. The normalization was reported through xd://report_issue.
- `/Users/robert.lee/.local/state/agent-memory/research/rust-cutover/ledger-mark.sh /Users/robert.lee/.local/state/agent-memory/research/rust-cutover/sharingrecovery-ported.txt tests/t2_sharing_recovery.rs`: corrected local run marked 1 ported / 18 partial.
- `grep -n 'unwrap()\|expect(\|\[.*\.\.' src/sharing_recovery.rs src/cli/helpers/sharing_recovery.rs`: no matches on the final refreshed code. There is no unsafe code in either module; Git hashes reuse existing digest crates and fixed-nibble hex indexing has a local invariant.

Lane implementation, ledger marking and the macOS finish line are complete: formatting clean, all-target check warning-free, own target green three times, and complete corrected-byte suite green. No Linux qualification is claimed. The 18 end-to-end recovery contracts remain explicitly partial until SharingBoundary (and, for five methods, memory_refresh) is registered at integration.
