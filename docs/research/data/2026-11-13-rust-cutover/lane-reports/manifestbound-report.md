# Manifest bound lane

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-manifestbound` (assigned detached baseline `c8c7feb5`). No Git writes, new crates, or subagents.

## Result and changed files

Signed release-v2 manifests now have a dedicated documented `MAX_MANIFEST = 1024 * 1024` bound. Small metadata remains bounded by `MAX_METADATA = 65536`; publisher signatures remain bounded by 4096 bytes. The manifest bound includes the signed command surface and the encoded trailing newline.

Changed repository files:

1. `src/release_distribution.rs`
2. `src/installation/plan.rs`
3. `src/updating.rs`
4. `src/bin/am_dev/release.rs`
5. `src/cache_recovery.rs`
6. `tests/t2_delivery.rs`
7. `tests/t2_installation.rs`

## Every changed production call site

- `release_distribution::parse_manifest`: new manifest-specific JSON entry point uses `MAX_MANIFEST` both before decoding and in `DecodePolicy.max_bytes`. A private `parse_bounded` helper retains the strict duplicate-key/nonfinite policies. Existing generic `parse` continues to use `MAX_METADATA`.
- `ReleaseManifest::encoded`: canonical manifest plus its newline is bounded by `MAX_MANIFEST`; this also bounds developer-builder manifest writes through `manifest.encoded()` before `write_new`.
- `release_distribution::verify_manifest_with`: raw manifest verification bound and subsequent JSON parsing use `MAX_MANIFEST` / `parse_manifest`.
- `release_distribution::probe_with`: first manifest download uses `MAX_MANIFEST`, and its untrusted tag-selection parse uses `parse_manifest`.
- `release_distribution::installed_with`: `regular(root/MANIFEST, MAX_MANIFEST)`; signature read remains 4096.
- `installation::plan::source_info`: signed source manifest readback uses `MAX_MANIFEST`.
- `updating::declared_commands`: signed command-surface manifest read uses `MAX_MANIFEST`.
- Developer release `register_providers`: canonical command-surface preflight uses `MAX_MANIFEST`; complete encoded-manifest overhead remains enforced by `ReleaseManifest::encoded`.
- Developer release `sign`: manifest readback before signature verification uses `MAX_MANIFEST`; binary signatures retain `MAX_BINARY` and signature files retain 4096.
- `cache_recovery::observe_with` nested `walk`: root manifest file size and descriptor read use `MAX_MANIFEST`; unrelated file reads keep `MAX_FILE`. Aggregate source inventory now accounts for `MAX_BYTES + MAX_CACHE_BYTES + MAX_MANIFEST + 4096` instead of two small metadata allowances.

Unchanged small JSON paths: generic release `parse`, GitHub projected API output (`MAX_GH_RESPONSE`), API/locator parsers, package version declarations, and compatibility fixtures retain `MAX_METADATA`.

`install.sh` was inspected and not changed: its only `am_download` callers fetch the binary and its signature, not the manifest. Its shared curl `--max-filesize` uses those independent caller-supplied limits.

## Tests

- Added signed verification of a command surface greater than 64 KiB but smaller than 1 MiB; the generic metadata parser still refuses those manifest bytes.
- Added exact-1-MiB successful manifest signing/verifying, one-byte-over refusal in parsing/verifying, and oversized manifest encoding refusal.
- Retained a 4097-byte signature refusal regression.
- Expanded selected-tag acquisition and cache-recovery fixtures above 64 KiB to exercise manifest download, parsing, signature verification, and cache qualification.
- Added cache observation refusal above `MAX_MANIFEST`.
- Added end-to-end signed bootstrap/install with a large manifest, installed-byte and receipt-hash readback, then oversized download refusal without changing installed state.
- Updated delivery manifest download bound and developer publisher test manifest readback to `MAX_MANIFEST`.
- Initial formatting checks found formatting differences; these were corrected with scoped edits. The first affected-target run found the new test compared insertion-ordered JSON values against canonical key order; it was corrected to compare typed `ReleaseManifest` values. No production behavior was relaxed.

Public API additions: `MAX_MANIFEST` and `parse_manifest`. No registration traits, CLI declarations, foundation changes, or obligation-ledger changes. Intended behavior delta: release-v2 manifests may grow from 64 KiB to 1 MiB; other metadata and signature limits are unchanged. LSP references were attempted, but the tool reported no language server for this worktree.

## Platform and verification

Observed platform: `aarch64-apple-darwin`, Darwin 27.3.0 (`xnu-13432.60.57~99/RELEASE_ARM64_T6050`), rustc 1.99.1 (`58cf8f9c3`, LLVM 23.1.1).

Verification command outputs are retained in `~/.local/state/agent-memory/research/rust-cutover/manifestbound-logs/`:

- `cargo fmt --check`
- `cargo check --offline --all-targets --features dev-tools`
- Three consecutive runs each of:
  - `cargo test --offline --features dev-tools --lib release_builder::`
  - `cargo test --offline --features dev-tools --test t2_delivery --test t2_installation --test t2_lifecycle`
- `cargo test --offline --no-fail-fast --features dev-tools`

Initial all-target compilation passed without warnings. Final verification results will be appended after the running command completes.

Tracker discovery: `awa status --json` reported a managed repository; a read-only GitHub search did not identify an unambiguous manifest-bound story. No issue/project phase was changed and no reconciliation receipt is claimed. Integration and tracker ownership remain with Main. Vox status reported the existing `omp-work` node Ready in room `kh57aq2zwdss`.
