# Layout history and private workspace lane

Base: `f7391170a0774f1589218025450346ff7e356f23`; worktree `/opt/agent-memory-rust-layout-history-workspace`, branch `lane/layout-history-workspace`.

## Delivery and files

Implemented the complete 46-line `layout_history.py` and 141-line `private_workspace.py` baselines, their actual helper declaration, and the `installation.MEMORY_ROUTES` `private` consumer route. No baseline function or branch is intentionally omitted. Python sources remain untouched as required for this lane.

New files:
- `src/layout_history.rs`
- `src/private_workspace.rs`
- `src/cli/helpers/private_workspace.rs`
- `tests/t1_layout_history_workspace.rs`
- `docs/research/data/2026-11-13-rust-cutover/lane-reports/layout-history-workspace-report.md`

Modified files:
- `src/lib.rs`: two module declarations only.
- `src/cli/helpers/mod.rs`: one module declaration only.
- `src/cli/table.rs`: `route("private", &helpers::private_workspace::HELPER)` in baseline route order.
- `tests/obligations/python-baseline-v1.json`: parent marked the private workspace obligation **partial** using `am-dev`; this is a parent-authored ledger change included in the delivery.

External handoff artifact: `/tmp/layout-history-workspace-obligation-ids.txt` contains the exact ledger ID. Current ledger status: **partial**, marked by the parent using `am-dev`, because the complete consumer scenario is present but ignored until the real security provider is integrated. Parent may mark ported after enabling/running that scenario.

No changes to crates, foundations, shared traits/error kinds, other domain implementations, integration branch, Git history, or tracker state.

## Public API and callers

`layout_history`:
- `check(repo, scope, project_id, runtime_root) -> Result<PathBuf>`.
- `recovery_receipt(directory, preview, canonical) -> Result<Value>`.
- `recovery_receipt_with_sources(directory, preview, canonical, Sources) -> Result<Value>`.
- `completed(directory, preview) -> Result<Option<Value>>`.

The checkout check uses the existing layout state path/binding guards, real principal resolution, validated project registry, bounded/owned Git query, and descriptor-pinned common-directory device/inode. It does not resolve current project/bank health, authorize writes, interpret bank prose, or create registry entries. Known completion binds both exact canonical encodings and requires actual boolean `verified` in both records. It returns the canonical historical receipt without touching memory.

`runtime_root` is explicit, matching the already-ported `layout_recovery_state::paths` contract. Future Rust callers in layout recovery/resolution/restore/recovery transactions must pass their executing runtime root. There were no existing Rust callers of either new module to migrate; the baseline Python references were located before implementation and remain historical specifications, not executable bridges.

`private_workspace`:
- `MAX_INPUT` = 16 MiB; `MAX_BINDING` = 4096 bytes.
- `workspace_root(repo) -> Result<(PathBuf, Value, PathBuf)>`.
- `create(repo)`, `create_with_sources(repo, Sources) -> Result<Value>`.
- `checked_directory(path) -> Result<(Descriptor, Identity)>`.
- `check_directory(path, &Identity) -> Result<()>`.
- `write_fd(&Descriptor, name, raw)`, `write_exclusive(directory, name, raw) -> Result<()>`.
- `read_binding(&Descriptor) -> Result<Vec<u8>>`.
- `put(repo, workspace, name, raw) -> Result<Value>`.
- `command_result(operation, Result<Value>) -> lifecycle::CommandResult`.

The held `Descriptor` replaces the baseline raw descriptor ownership. Future sharing-page-export callers can use the same checked-directory/write/recheck primitives directly. No fake providers or new provider interfaces were added.

`am private create --repo PATH` and `am private put --repo PATH --workspace ID --name NAME` are real typed handlers. Put consumes at most 16 MiB + 1 from stdin, preserves accepted raw bytes, and emits the baseline compact-default JSON/newline success/refusal envelopes, including capacity details and repair text. Both report `canonical_writes_performed: false`.

## Isolation, trust and bytes

Workspace paths reuse real scope resolution and `bank_writes::{bank_path,state_paths}`: `<state>/writes/<bank-path-sha256>/private-workspaces/<uuid-hex>`, outside the repository. Binding bytes use `developer_identity::encoded` (sorted keys, indent 2, UTF-8, finite output, trailing newline). Input uses its strict duplicate-key decoder without re-encoding; object/array passthrough still accepts baseline non-finite tokens.

Directories remain exactly 0700 and newly created records exactly 0600. Existing permissions are never repaired. Descriptor-relative creation is exclusive/no-follow, synchronizes file and directory, and never replaces existing records or follows filename symlinks. Binding reads require bounded, effective-owner, regular, single-link, exactly 0600 files. Directory device/inode/owner/mode and scope are checked before and after put; original binding bytes are compared after writing. Concurrent replacement preserves the written evidence in the pinned original directory, rather than writing into the replacement.

## Dependencies and wiring

No registration point belongs to these implementers, so no new pending line or removal from `tests/wiring-pending.txt` is necessary. Existing production registrations already wire scope binding and project observation/writer operations. The new route reaches those real implementations directly.

The genuinely unavailable base dependency is **security**, specifically `repository_config::SecurityScan`. Ordinary private scope resolution and layout state path resolution correctly propagate its typed refusal at this base. The security lane owns its implementation and composition registration. No substitute scanner, fake identity context, mock recovery/frontier, or default success is used here. A recovery-frontier provider is not required for this lane's real project-registry binding fixture: it calls real `project_identity::preview/apply`, not the higher-level scope mutation workflow.

## Contract tests and obligations

Focused target: `t1_layout_history_workspace`.

Five unignored deterministic real-filesystem tests cover:
- Historical receipt fields, fixed independent encoded-byte/hash vectors, missing completion, exact replay, request-ID reuse, all linkage/verification refusals, and changed canonical receipt.
- Noncanonical/unsafe retained evidence, modes, hardlinks, symlinks, malformed direct calls/path traversal.
- Workspace ID/filename grammar, reserved filename, scalar input, duplicate keys, malformed/non-UTF-8/BOM input, 16 MiB overflow, exact refusal envelope.
- Exclusive writes, exact modes, no overwrite/symlink redirection, and deterministic held-descriptor directory replacement retaining original written bytes.
- Binding read bounds, permission/type/link restrictions, and missing/symlink bindings.

Two additional ungated regressions cover strict-decoded capacity refusal output (one repair key, capacity-specific repair text, and baseline dictionary replacement preserving field order), and the actual `am private put` binary's invalid-name/16 MiB-overflow exit-1 envelopes before any security/scope dependency or filesystem mutation.

Two full-dependency tests are present with precise ignore reasons:
- `explicit_private_input_and_refusals_through_am_private`: exact baseline obligation `tests/contracts/test_private_workspace.py::PrivateWorkspaceContract.test_explicit_private_input_and_refusals`. Re-executes in an isolated private HOME/config/state environment, registers production providers only, creates a real Git bank/developer/project binding, invokes the actual `am private` binary, checks exclusive/path/JSON/binding refusals, accepted array/non-finite/raw bytes, maximal filename and inclusive 16 MiB stdin boundaries, deterministic directory replacement, and unchanged canonical repository/index/HEAD/feature/config bytes and modes. Ignores only missing security registration.
- `historical_checkout_check_ignores_bank_health_but_requires_exact_registered_identity`: real principal/project binding and checkout query; oversized/current malformed prose and interruption do not invalidate historical checkout identity; wrong project/key/location, lock, and unregistered checkout refuse without repository mutation. Ignores only missing security registration.

The baseline's monkeypatched `write_fd` race is represented by the same deterministic real-descriptor write, real directory rename/replacement and identity recheck, without adding production injection hooks. Full layout recovery/restore command-level historical-retry obligations belong to their downstream owners and are not falsely claimed by these helper tests.

## Declared deltas

- Explicit executing-runtime root in `layout_history::check`, inherited from the existing layout-state API; no durable format change.
- Rust typed arguments replace Python dynamic argument exceptions. Malformed direct historical calls receive typed corrupt-request refusals, including recovery IDs containing path separators/NUL.
- Shared strict UTF-8/BOM/surrogate/depth codec refusals remain the already-declared cutover deltas.
- Binding opens are nonblocking and bounded; foundation stable-read checks may refuse concurrent in-place mutation earlier than Python. FIFOs cannot hang the helper. New directory creation additionally synchronizes its parent. Only new inodes receive exact-mode enforcement.
- Filename/binding-open OS diagnostics reuse the existing baseline-style formatter; other foundation I/O diagnostics retain the foundation's platform rendering. Named meaningful refusal strings, JSON shapes and durable bytes are preserved.

## Observed parent verification and independent acceptance

Host: Linux x86_64, kernel `7.0.11-76070011-generic`, `/tmp` tmpfs, `/bin/sh` = `/usr/bin/dash`; rustc `1.94.1 (e408947bf 2026-03-25)`, cargo `1.94.1`.

The implementation worker ran no build/test/formatter/Python command. Parent verification after all corrections:

- `cargo fmt --check`: pass.
- `cargo check --offline --all-targets --features dev-tools`: pass, no warnings.
- `cargo test --offline --features dev-tools --target <triple> --test t1_layout_history_workspace`: **7 passed, 0 failed, 2 security-dependency ignored**, three consecutive runs for each of `x86_64-unknown-linux-gnu` and `x86_64-unknown-linux-musl`.
- `cargo test --offline --features dev-tools --target <triple> --no-fail-fast`: **545 passed, 0 failed, 26 ignored** across 27 top-level harnesses on each target. An initial count of 568 incorrectly included nested fixture subprocess summaries; this is the corrected harness-only total, with no rerun or source change. The ignored count includes existing dependency-gated tests; it is not a claim of complete cutover coverage.
- Actual GNU binary `am private --help`: exit 0, displays the real `create` and `put` subcommands. The active binary regression also proves exact invalid-name and 16 MiB overflow refusal envelopes without a security provider.
- Required production panic scan: no matches in the three new production files; the changed capacity merge was scanned again after correction.
- Parent `am-dev obligations mark --status partial --ids-file /tmp/layout-history-workspace-obligation-ids.txt`: pass. The one authored baseline obligation remains **partial**, not ported.

Initial offline compilation was blocked by uncached locked `const-oid 0.10.3`; `cargo fetch --locked` resolved that prerequisite. Initial all-target compilation exposed integration-test imports of crate-private lifecycle helpers; local test helpers corrected them without widening production visibility. Initial formatting differences were corrected through edits. Those failed logs are retained, not presented as successful runs.

Independent `anthropic/claude-opus-5-5:high` reviewer `ReviewWorkspaceHistoryWave` initially required changes for the private test imports, duplicate `repair` in capacity JSON and inaccurate ledger file list. All were corrected. Targeted re-review: **ACCEPT**, source-only, no P0/P2 remaining. Capacity details now replace existing keys in place; an active regression strict-decodes the output and checks the actual capacity repair text and key order.

Proof directory: `/tmp/agent-memory-layout-history-workspace-proof/`, including exact commands, platform, compile/format logs, all six focused runs, both full-suite logs and actual help output.

Acceptance limits remain explicit: the two real bound-checkout/create/put scenarios await the production security provider at this frozen base and must be enabled at integration before marking the obligation ported. The deterministic held-descriptor replacement test covers the underlying race primitives, not an injected concurrent replacement during `put` itself. Native harness, installed release, macOS and aarch64 qualification are not claimed here. The integration owner supplies the additional platforms and records the candidate/verdict through awa on #249; this lane does not advance whole-story acceptance.
