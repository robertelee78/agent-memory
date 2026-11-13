# Delivery lane report

## Result and integration boundary

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-delivery`.

Release-v2 verification/acquisition, bootstrap-byte binding, opaque-cache qualification/recheck, the developer release builder/publisher, shared manifest encoding, and the schema are implemented. Final verification is clean: `cargo fmt --check`; `cargo check --offline --features dev-tools --all-targets` (no warnings); `cargo test --offline --features dev-tools --test t2_delivery` passed **31/31 three consecutive times**.

As expressly directed by Main, the developer executable refuses until its production parser/declaration and migration providers are registered. This is a typed `release_distribution::Error::MissingProvider`, not a weaker inventory-only build. Main must wire `release_builder::register_providers(Some(Providers { surface: Arc<actual CLI CommandSurface>, migration: Arc::new(migration_contract::Bundled), command_surface: actual CLI parser-table declaration }))` at executable startup. There is **no `--command-surface FILE` option**. The reusable build/publish functions already accept these providers explicitly and always run the runtime's full artifact preflight for all four binaries. Tests exercise that library preflight with real portable fixtures and `migration_contract::Bundled`.

No Python, network, awa/Vox or Git-write commands were run. The real pinned signing key and published GitHub endpoints were not available/used; actual production build/publication and bootstrap/adoption qualification remain Main's release qualification, not claimed here.

## Owned files created/modified

- `Cargo.toml`: empty `dev-tools` feature; `am-dev` binary at `src/bin/am_dev/main.rs`, `required-features = ["dev-tools"]`; approved pure-Rust flate2 dependency.
- `Cargo.lock`: resolved the approved dependency offline.
- `src/lib.rs`: registers `cache_recovery`, `release_distribution`, and feature-gated `release_builder` (`#[path = "bin/am_dev/release.rs"]`).
- `src/release_distribution.rs` (new).
- `src/cache_recovery.rs` (new).
- `src/bin/am_dev/main.rs` (new).
- `src/bin/am_dev/release.rs` (new; reusable developer-only library module as well as developer executable support).
- `skills/agent-memory/schemas/release-v2.json` (new).
- `skills/agent-memory/keys/allowed-signers` (Main-authorized one-line change).
- `tests/t2_delivery.rs` (new).
- `src/fs/descriptor.rs`: integration copy plus **one new T0 primitive**, `Descriptor::create_directory(&self, name: &OsStr, mode: u32) -> io::Result<Descriptor>`. It exclusively creates a child, pins/rechecks its inode, and chmods only that newly created inode. Tests cover exclusive creation, exact 0755 mode and bounded listing. The original duplicate readdir implementation was removed; this worktree uses the integrated shared `entries_bounded` implementation.

Exact allowed-signers line (followed by LF):

```text
release@agent-memory namespaces="agent-memory.release,agent-memory.release-binary" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB+C//uSRKmScS5hTki796yvLR2oOSn2IKHLkr1RWE2y
```

### Non-owned integration prerequisites copied on Main's instruction

These are synchronization copies, **not additional changes to land over the owners' files**:

- `src/artifact.rs` — copied complete integrated implementation; supersedes this lane's temporary `runtime_digest` implementation. Preserve the artifact owner's version at integration.
- `src/pack_validation.rs`, `src/process.rs`, `src/handoff.rs`, `src/records.rs`, `src/scopes.rs`, `src/sharing_limits.rs`, `src/developer_identity.rs`, `src/host.rs`, `src/local_registry.rs`, `src/fs/lock.rs`, `src/fs/resolve.rs`.
- New in this worktree from integration: `src/bank_contract.rs`, `src/bank_layout.rs`, `src/bank_writes.rs`, `src/migration_codecs.rs`, `src/migration_contract.rs`, `src/omp_installation.rs`.
- Existing tests synchronized: `tests/t0_fs.rs`, `tests/t0_host.rs`, `tests/t0_unicode.rs`, `tests/t1_a2.rs`.
- New copied tests: `tests/t1_bank.rs`, `tests/t1_migration.rs`, `tests/t2_bootstrap.rs`, `tests/t2_omp_install.rs`.
- `src/fs/descriptor.rs` and `src/lib.rs` were also synchronized before reapplying the narrowly scoped additions above.

Copies came from `/opt/agent-memory-worktrees/rust-v015-int` as Main directed. Some initial bulk edit attempts were refused for omitted reader ranges; those refusals were resolved by reading the exact omitted ranges and reapplying, then compiling the integrated result. Do not infer any owner-file delta from those intermediate attempts.

## Exact release-v2 manifest contract

Top-level JSON object has **exactly these 11 fields**, with no additional fields:

1. `schema`: constant `agent-memory/release-v2`.
2. `repository`: constant `robertelee78/agent-memory`.
3. `tag`: `v` plus `version` (at most 65 bytes).
4. `version`: stable decimal `X.Y.Z`, each component `0` or nonzero-leading decimal; at most 64 bytes.
5. `source_commit`: 40 lowercase hexadecimal characters.
6. `archive`: constant `agent-memory.tar.gz`.
7. `archive_size`: positive JSON integer, at most 17,825,792 bytes (16 MiB + 1 MiB archive overhead).
8. `archive_sha256`: 64 lowercase hexadecimal characters.
9. `artifact_digest`: **portable-pack-only** inventory digest.
10. `targets`: object with exactly the four supported triples; each value has exactly `size` (positive JSON integer, at most 67,108,864 bytes), `sha256` (64 lowercase hex), and `artifact_digest` (64 lowercase hex).
11. `command_surface`: opaque finite JSON object produced by the CLI's parser table. Its bytes are covered by the manifest signature; this lane does not interpret or execute it.

Triples: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`.

### Canonical encoding

`ReleaseManifest::encoded()` uses recursive sorted-key encoding, compact `,` and `:` separators, ASCII escaping, finite numbers only, and **exactly one trailing LF**. This is the T0 codec's `SORTED_COMPACT_ASCII` with `allow_nan = false`, then LF. The manifest signature covers those exact emitted bytes. Parsing accepts signed noncanonical whitespace/key order as the baseline did, but rejects duplicate keys, nonfinite metadata, BOM/non-UTF-8 and metadata above 65,536 bytes.

Inventory values are objects mapping pack-relative names to `{"sha256": LOWER_HEX, "mode": "0o644" | "0o755"}`. Inventory digests use the same sorted/compact/ASCII encoding **without** a trailing LF. The single runtime-digest definition is `artifact::runtime_digest(files, binary_sha256, 0o755)`: pack inventory plus `bin/am` with its sha256 and mode `0o755`. Both builder and verifier call this function. Same-version runtime identity is `targets[triple].artifact_digest`, not the pack-only digest.

### Assets, trust and bounds

- Tag `vX.Y.Z`; assets `agent-memory.tar.gz`, `release-manifest.json`, `release-manifest.json.sshsig`, `am-<triple>`, `am-<triple>.sshsig`; builder additionally publishes tracked `install.sh`, as baseline publisher did.
- First manifest may come from `releases/latest/download/release-manifest.json`. Its syntactically constrained tag selects only the tag-pinned signature URL; verification then authenticates the tag. All subsequent binary-runtime downloads use the authenticated tag-pinned base. No mutable-latest second fetch.
- Pinned signers are `include_str!` from the single key file. Manifest verification passes exactly `-n agent-memory.release`; binary verification passes exactly `-n agent-memory.release-binary`. Both cross-namespace negative tests pass.
- `release-v1` is recognized only for the fixed re-bootstrap/adoption refusal; it is never installed.
- `bind_bootstrap_binary` checks compiled version and selected triple's byte size/sha256 at the supplied path, returns those same verified bytes, and does not download the binary again.
- Portable assets: 2 MiB each / 16 MiB total / 4096 entries. Separate binary: 64 MiB. Signature: 4096 bytes. Metadata: 65536 bytes. Inflated USTAR including headers: 20 MiB (`MAX_BYTES + MAX_ENTRIES*1024`).
- Gzip reads are streaming-capped; complete accepted streams validate CRC/ISIZE. Trailing bytes, concatenated streams, invalid USTAR checksums, noncanonical/absolute/parent paths, duplicate entries, PAX/links/devices and unsafe modes refuse before extraction. Directory mode is 0755; destination root is private 0700. Builder gzip/tar mtime, uid/gid, names and order are fixed; reproducibility is tested.

## Receipt contract for the installation lane

The Rust-era receipt `source` carries `target` (one of the four compiled triples) next to `version`. `source.artifact_digest` is the per-target runtime digest. The manifest's top-level digest stays pack-only, and receipt `artifact_files` is the authenticated pack-only inventory. Cache qualification authenticates `bin/am` separately and binds it into the runtime digest and whole-source fingerprint. Its transient `files` result is the clean portable pack inventory, not a second binary copy.

`source` equality includes `kind`, `path`, `pack`, `remote`, `ref`, `revision`, `dirty`, `version`, `artifact_digest`, `compatibility`, `release_manifest_sha256`, `signature_namespace`, `signature_principal`, and `target`. JSON equality is canonical structural equality, not insertion-order equality. Python-era receipts without `source.target` cannot receive cache qualification; they must take the explicit adoption/refusal path. A release-v1 manifest produces the designed re-bootstrap guidance.

The shell argv agreed with Bootstrap is `"$dir/am-<triple>" install --bootstrap-binary "$dir/am-<triple>" <user args...>`, run as a child so shell cleanup follows completion.

## Public API and seams

- Shared `ReleaseManifest`, `BinaryIdentity`, `ReleaseObservation`; strict parsing, `value`, `encoded`; canonical/inventory hashing; `compare_command_surfaces(declared, expected)` returns a typed refusal unless canonical object encodings agree.
- Production verification: `verify_manifest`, `verify_binary_signature`, `bind_bootstrap_binary`, `package_identity`, `installed`, `probe`, `acquire`, `download`.
- Explicit independent trust/test variants: `verify_manifest_with`, `verify_signature_with`, `installed_with`, `probe_with`, `acquire_with`, `download_with`, `download_with_environment`. `file://` is available only through explicit `DownloadOptions.allow_file`; production defaults require HTTPS and HTTPS-only redirects.
- `Error` distinguishes fixed contract refusals, unexpected IO failures, supervised-process failures, boxed structured `DownloadError`, and missing release providers. Download errors retain `download-failure-v1` fields and repair guidance; acquisition does not flatten/discard that evidence.
- Cache `Installation` trait: `fault(point)` and `inspect_package(pack)`. Future production implementer is **installation**; its inspection delegates to the actual artifact/migration/parser providers, never a candidate executable. `register_installation(None)` causes a named refusal; explicit `observe_with`, `qualify_with`, `recheck_with` support independent tests. Public `Observation`/`Qualification` preserve source pins, clean files, receipt/source, and the opaque-cache summary. No cache is parsed, unmarshalled, executed, deleted or reused.
- Developer-only `release_builder::build(inputs, output, surface, migration, command_surface)` and `publish(inputs, notes, integration_branch, surface, migration, command_surface)`; `preflight` calls `artifact::inspect_runtime_with` for every target. `Providers`, `register_providers`, `registered_providers` provide the explicitly pending CLI wiring seam.
- Publisher ports draft lookup/resume, bounded annotated-tag peeling, source/origin/integration/main-policy checks, immutable existing-asset comparison, upload only missing draft assets, complete readback, publication/tag/latest/public acquisition checks, nonforced main activation, and default bootstrap/runtime-tree/public-bootstrap readback. All external commands use T0 process supervision. Lookup calls use 60 s, general publisher commands 180 s, signature verification 20 s, curl the remaining absolute 30 s budget.

## CLI declarations

Baseline release.py positional `operation` choices were `build`, `publish`, `render-bootstrap`; optional `--private-key PATH`, `--output PATH`, `--notes-file PATH`, `--integration-branch STRING` (all default None); key required for build/publish; output required for build; ordinary notes file required for publish. No mutually exclusive argument group.

Rust developer CLI is `am-dev release build|publish`; the same key/output/notes/integration options remain, with `--root PATH` (default build checkout) and repeatable `--binary TRIPLE=PATH` (exactly all four target keys). The signed command declaration comes only from registered CLI parser-table providers. `render-bootstrap` is deliberately removed: the Rust bootstrap is the separately owned tracked POSIX shell, not Python verifier text interpolation. Neither runtime module had an argparse CLI to port.

## Tests and obligations

`t2_delivery` uses private canonicalized `/tmp` roots, disposable Ed25519 keys, real ssh-keygen verification, real file:// curl transfers, real filesystem observation and process deadlines. Transport-fault tests substitute a private shell curl fixture through an explicitly supplied child environment; they never mutate global PATH or contact a host.

Fully covered module-level obligations include signature rejection/cross-namespace rejection, strict manifest shape/bounds, exact tag selection, canonical declaration comparison, bootstrap-byte/version binding, reproducible archive roundtrip/modes, malformed/traversal/link/PAX/duplicate/checksum/gzip trailer/multimember/inflation refusal, ordinary-file/link/mode bounds, private download diagnostics/redaction, discarded partial responses, exact retry policy, permanent HTTP precedence, deadline and SIGXFSZ handling, and explicit missing-provider refusal.

Legacy obligation comments immediately precede the relevant test functions:

- `DownloadDiagnostics.test_recovery_discards_partial_bytes` — fully ported.
- `DownloadDiagnostics.test_private_bounded_redacted_evidence` — fully ported.
- `DownloadDiagnostics.test_no_retry_for_permanent_errors` — fully ported across permanent failures and size-limit controls.
- `DownloadDiagnostics.test_http_retry_and_deadline` — fully ported with an explicitly injected 200 ms deadline.
- `DownloadDiagnostics.test_resource_signal_not_retried` — fully ported with a real owned-child SIGXFSZ.
- `ReviewRegressions.test_diagnostic_redaction_normalizes_before_matching` — fully ported.
- `ReviewRegressions.test_http_permanent_status_wins_over_transient_curl_exit` — permanent-status precedence ported (403 fixture; implementation covers 429/403/404).
- `SignedPublisherContract.test_unsafe_signing_key_paths_refuse_before_output_or_key_use` — custody refusals ported through the reusable builder with supplied providers; no output is created.
- `DownloadBootstrap.test_invalid_signature_is_not_retried` — cryptographic/module boundary ported; actual bootstrap/lifecycle invocation deferred to Bootstrap/installation.
- `CacheUpdateRecoveryContract.test_same_version_preview_fresh_acquisition_exact_backup_and_clean_noop` — qualification/read-only summary/no-cache refusal and changed-cache recheck ported; preview/activation/exact backup/no-op CLI portions require installation/updating.
- `CacheUpdateRecoveryContract.test_python_created_group_writable_cache_directories_below_private_source_recover` — qualification beneath the private barrier ported; public activation portions require installation/updating.
- `CacheUpdateRecoveryContract.test_exact_count_and_byte_limits_allow_recovery_but_next_cache_refuses` — exact 256/257 count and 16 MiB/+1-byte qualification boundaries ported, including round-tripped sorted receipt JSON; replacement/backups require installation/updating.
- `CacheUpdateRecoveryContract.test_unqualified_payload_cache_types_and_bounds_preserve_every_target` — wrong module/tag/mode, links, payload/binary/receipt drift qualification refusals ported; full public target/bank snapshots require installation/updating.
- `CacheUpdateRecoveryContract.test_late_signed_payload_or_other_target_change_never_receives_race_qualification` — final source observation writer refusal/preservation ported; other owned-target activation race requires installation/updating.

Explicitly deferred public integration obligations (not claimed migrated by these tests): remaining `SignedReleaseLifecycle` methods/branches; publisher `test_one_command_first_publish_exact_resume_and_harmless_descendant` and `test_wrong_tags_and_incompatible_or_racing_main_never_claim_default_activation` (publication implementation exists, but their gh-authority simulator/production signing qualification has not been ported/run here); cache older-release activation, copy-client drift, reachable-writable public install setup, interrupted activation recovery, concurrent-writer retry convergence and all source-backup/consumer-bank preservation branches; `DownloadBootstrap` authenticated install/failure/preview and console guidance cases. These depend on the public Bootstrap/installation/updating/CLI integration and, for publication, an independent gh authority apparatus. The download diagnostic-retention failure branches are implemented but their baseline monkeypatch-based failure-injection tests (`DownloadDiagnostics.test_retention_failure_preserves_primary_error`, `ReviewRegressions.test_report_policy_failure_preserves_primary_error`) are not forced by this lane's tests.

No baseline runtime function is intentionally left unimplemented: release_distribution's parsing, signature/package verification, extraction, location/redaction/retention/failure/retry transport, probe/acquire/installed behavior and cache_recovery's observe/qualify/recheck are ported. The developer build/publish functions are implemented with required providers. Remaining executable production-provider registration is the exact Main-approved integration dependency named above; old `render`/`render-bootstrap` are intentionally obsolete, not preserved as compatibility shims.

## Declared deltas

- Release-v2, per-target runtime identity, separately signed binary assets, parser-table declaration and explicit release-v1 re-bootstrap refusal are the requested cutover contracts, not silent v1 acceptance.
- A gzip stream must be one complete member with no trailing bytes, as Main required; tar extensions/ambiguous paths are refused deterministically. Inventory recursion is bounded at 64 components.
- JSON/text errors use the strict T0 codec rather than interpreter-dependent decoding/traceback behavior; UTF-8 only, no BOM/nonfinite/duplicate metadata. Signature and meaningful domain refusals retain the baseline wording where unchanged by v2.
- Diagnostic redaction is intentionally more conservative: a secret/environment-bearing line is wholly redacted, and any URL/userinfo/path-bearing token is replaced. This can suppress more harmless diagnostic text than the baseline regex, while retaining reason/curl_exit/http_status/guidance and never admitting payload bytes.
- Public `source.target` is required for Rust cache qualification; legacy receipts/manifests take adoption/refusal, never implicit qualification.
- Builder uses tracked `git ls-tree`/`git cat-file` bytes (bounded) rather than trusting Git archive PAX headers; emitted portable tar/gzip is deterministic USTAR. Python source/bytecode is refused. The runtime's full semantic artifact preflight remains mandatory.
- The Python-embedded bootstrap rendering operation is removed; the separately reviewed tracked POSIX bootstrap is copied as the release bootstrap asset.
- IO errors that the baseline propagated remain distinct unexpected IO errors, not a blanket ContractError conversion.

## Verification history and final commands

During implementation, cargo check exposed missing IO conversions; these were resolved with the local typed Error::Io, not a shared blanket conversion. A deadline test exposed owned-child SIGKILL evidence incorrectly overriding `deadline`; that mapping was fixed. Missing integrated artifact helpers prompted the authorized full prerequisite synchronization, after which all-target compilation passed.

Final exact commands, from the delivery worktree:

```text
rustfmt --edition 2024 --config skip_children=true src/release_distribution.rs src/cache_recovery.rs src/bin/am_dev/main.rs src/bin/am_dev/release.rs src/fs/descriptor.rs tests/t2_delivery.rs
cargo fmt --check
cargo check --offline --features dev-tools --all-targets
cargo test --offline --features dev-tools --test t2_delivery
cargo test --offline --features dev-tools --test t2_delivery
cargo test --offline --features dev-tools --test t2_delivery
grep -n 'unwrap()\|expect(\|\[.*\.\.' src/release_distribution.rs src/cache_recovery.rs src/bin/am_dev/main.rs src/bin/am_dev/release.rs
```

Results: formatting clean; check clean/no warnings; tests 31/31 in 1.13 s, 1.09 s, 1.22 s. Earlier pre-provider target passed 20/20; the final target supersedes that evidence. No unrelated tests were executed.

Panic-scan justification: no unwrap/expect remains in owned runtime/builder modules. Remaining slicing is confined to the USTAR codec: the parsing loop proves a full 512-byte header; field endpoints are checked or constant within that header; the field terminator position is found inside the slice or equals its length; the member range is checked before slicing. Encoding checks leaf/prefix lengths and fixes numeric widths/allowed mode and size before copying into its fixed 512-byte header. The builder's `[..]` scan match is slice-pattern syntax; its sole `portable[0]` access is guarded by the immediately preceding short-circuit `len() == 1` check and documented invariant. Cache maps/paths use checked lookups rather than input-indexed panics.

Approved archive dependency: `flate2 = { version = "=1.1.11", default-features = false, features = ["rust_backend"] }`. Resolved transitive graph: `crc32fast 1.5.5` -> existing `cfg-if 1.0.5`; `miniz_oxide 0.9.1` -> `adler2 2.0.1`, `simd-adler32 0.3.10`. No zlib/C backend, runtime_detection or new consumer executable dependency. Cargo.lock is retained for Main to commit; no Git writes were performed here.
