# Bootstrap lane report (v0.0.15 Rust cutover)

Worktree: `/opt/agent-memory-worktrees/rust-v015-lane-bootstrap` (snapshot `5693675`). No Git writes.

## Files
- `install.sh` — rewritten; pure POSIX sh, no embedded program (all Python removed).
- `.github/workflows/contracts.yml` — rewritten for the Rust tree.
- `tests/t2_bootstrap.rs` — new test target (26 tests).
- `skills/agent-memory/keys/allowed-signers` — one-line change on Main's instruction (same bytes Delivery uses):
  `release@agent-memory namespaces="agent-memory.release,agent-memory.release-binary" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB+C//uSRKmScS5hTki796yvLR2oOSn2IKHLkr1RWE2y`
- No `src/`, `Cargo.toml`, `Cargo.lock`, T0 or `error.rs` edits. No traits introduced.

## install.sh behaviour
1. Whole program inside `if :; then … fi` (truncated download runs no prefix), `set -eu`, every expansion quoted, no `local`, `eval`, `awk`, `sed`, Perl, Python, hashing tool or JSON parsing. `sh -n`/`dash -n` and `shellcheck` (sh and dash) clean.
2. `--help`/`-h` anywhere in argv: help, exit 0, before anything else (no uname, curl or ssh-keygen needed).
3. Host classified from `uname -s`/`uname -m` into constant `os`/`arch` tokens (`darwin|linux|freebsd|openbsd|netbsd|dragonfly|sunos|aix|haiku|windows|other`, `aarch64|x86_64|x86|arm|riscv64|ppc64le|ppc64|s390x|other`; uname bytes never echoed).
4. Option syntax checked exactly as the baseline (`--opt value` and `--opt=value`; value non-empty and not `--…`; development-source vs remote/ref conflict) → fixed JSON, exit 2, no download.
5. Unsupported target → fixed JSON naming os/arch tokens and the four supported triples, exit 1.
6. `command -v curl`, then `command -v ssh-keygen` → fixed JSON, exit 1.
7. `umask 077`; `mktemp -d "$TMPDIR/am-bootstrap.XXXXXXXX"` (non-absolute/unset TMPDIR → `/tmp`), must be a real directory, `chmod 700`. EXIT trap removes it; HUP/INT/TERM traps exit 129/130/143 through the EXIT trap.
8. `curl` `am-<triple>` (limit 64 MiB) then `am-<triple>.sshsig` (limit 4096) from `AM_RELEASE_BASE` = `https://github.com/robertelee78/agent-memory/releases/latest/download/`, with `--disable --fail --silent --location --proto =https --proto-redir =https --max-redirs 5 --max-time 60 --connect-timeout 5 --max-filesize LIMIT --header 'Accept: application/octet-stream' --write-out '%{http_code}'`, inside a subshell with `ulimit -f`; final size checked with `wc -c`. Retry/classification = baseline `download()`: 3 attempts; retry curl {5,6,7,16,18,28,52,55,56,92} when HTTP status absent or <400, or curl 22 with HTTP {408,500,502,503,504}; size-limit for curl 63, SIGXFSZ (153) or size > limit; process-start for 126/127; process-terminated for other >128; baseline reason vocabulary and repair guidance strings.
9. `ssh-keygen -Y verify -f <private allowed-signers> -I release@agent-memory -n agent-memory.release-binary -s <sig> < <binary>`, allowed-signers written from the pinned constant `AM_ALLOWED_SIGNERS` (test asserts it equals `skills/agent-memory/keys/allowed-signers` byte for byte, plus LF).
10. `chmod 700` binary, run as a child: `"$dir/am-<triple>" install --bootstrap-binary "$dir/am-<triple>" <user args…>` (argv agreed with Delivery), then exit with its status; the EXIT trap removes the directory.

Failure line (stdout, one line, exit 1; usage exit 2):
`{"schema":"agent-memory/bootstrap-failure-v1","stage":S,"reason":R,"os":O,"arch":A,"curl_exit":N|null,"http_status":N|null,"activation_accepted":false,"repair":G}`
- stages: `usage` (`unknown-option`, `missing-value`, `conflicting-sources`), `target` (`unsupported-target`), `dependency` (`missing-curl`, `missing-ssh-keygen`), `workspace` (`temporary-directory`), `binary-download` / `signature-download` (baseline download reasons: `transport failure`, `HTTP rejection`, `receive failure`, `size-limit`, `certificate validation failure`, …), `signature-verification` (`invalid-signature`).
- All strings come from case tables in the script; integers are `$?` and a `[1-5][0-9][0-9]` HTTP status (else `null`).

## Baseline install.sh options and what became of them
| Baseline option | Now |
|---|---|
| `--help`, `-h` | Handled by the shell; exit 0, no download, no dependency; help text kept (Python line removed, bootstrap/requirements/quiescence lines added; still contains `am update`). |
| `--check`, `--dry-run`, `--json`, `--verbose`, `-v`, `--no-guidance`, `--omp-peers-upgraded` | Flags; syntax-checked, forwarded verbatim to `am install`. |
| `--mode`, `--presets`, `--target-dir`, `--bin-dir`, `--omp-agent-dir`, `--omp-layout-quiescence`, `--layout-quiescence` | Value options (both forms); syntax-checked, forwarded verbatim. |
| `--development-source PATH` | Forwarded. Contract change: baseline skipped the download and ran the source tree's own installer unverified; now the shell always downloads and verifies the release binary, and the binary handles the development source (design: prebuilt `am` inside the source). |
| `--remote URL`, `--ref BRANCH` | Forwarded. Contract change: the shell no longer runs `git clone`; the binary decides (design: managed channels adopted read-only, update refuses). |
| development-source vs remote/ref exclusion | Kept in the shell, exit 2, `conflicting-sources`. |
| Unknown option → `am: Unknown install option: X (use --help)`, exit 2 | Fixed JSON `unknown-option`, exit 2 (no interpolation of the option). |
| Python 3.11 probe of every PATH entry | Removed. |
| Test override mechanism | Baseline had no runtime override; fixtures rewrote constants (`API`, `ASSETS`, `PUBLIC_KEY` in `release_distribution.py`) and re-rendered the script. Kept as: three constant lines `AM_RELEASE_BASE`, `AM_PROTOCOLS`, `AM_ALLOWED_SIGNERS`, rewritten only in a private copy. No environment variable or option can change them (tested: env `AM_RELEASE_BASE`/`AM_PROTOCOLS`/`AM_ALLOWED_SIGNERS` are ignored and the production URL is requested). The shipped `=https` restriction refuses `file://` (tested). `curl --disable` ignores `.curlrc`; curl's own CA env (`CURL_CA_BUNDLE`) is still honoured as before. |

## Tests (`tests/t2_bootstrap.rs`)
Every scenario runs under `/bin/sh` plus `dash` and `bash` when present, with a private 0700 root under canonical `/tmp`, PATH limited to a private tool directory (symlinks to uname, mktemp, rm, chmod, wc, sleep, curl, ssh-keygen, cmp, cat, ls, grep), private HOME and a `TMPDIR` containing spaces; a fake `uname` pins Linux x86_64 by default; throwaway `ssh-keygen` ed25519 key; stand-in binary is a shell script recording argv, directory/binary modes and byte equality of `--bootstrap-binary` with `$0`. Every refusal asserts exact stdout bytes, exit code, binary not run and scratch removed.

Tests: `pinned_allowed_signers_equal_the_committed_key_file`, `script_is_posix_sh_without_interpreters_hashing_or_eval`, `verified_binary_runs_with_its_own_path_and_user_arguments_then_scratch_is_removed`, `piped_script_runs_the_verified_binary_and_cleans_scratch`, `truncated_script_executes_no_prefix`, `every_supported_host_selects_its_own_signed_binary`, `real_uname_selects_the_host_triple`, `unsupported_target_refuses_with_exact_constant_json`, `missing_curl_refuses_before_any_scratch`, `missing_ssh_keygen_refuses_before_any_download`, `unusable_temporary_directory_refuses`, `help_and_invalid_options_need_no_download_or_dependencies`, `every_baseline_option_is_forwarded_verbatim`, `binary_exit_status_propagates_and_scratch_is_removed`, `termination_while_the_binary_runs_still_removes_scratch`, `missing_assets_fail_as_downloads_without_running_anything`, `shipped_protocol_restriction_refuses_non_https_and_environment_cannot_redirect`, `signature_from_another_key_is_refused_once_without_running`, `manifest_namespace_signature_is_refused_for_the_binary`, `truncated_or_altered_binary_is_refused`, `oversized_signature_is_bounded`, `permanent_download_failures_are_not_retried`, `permanent_http_status_wins_over_transient_curl_exit`, `file_size_signal_is_a_size_limit_and_not_retried`, `transient_failure_discards_partial_bytes_and_recovers`, `retryable_http_status_is_retried_and_attempts_are_bounded`.

## Obligations
Fully ported (bootstrap part is the whole obligation):
- `tests/contracts/test_installer.py::OneLineBootstrapContract.test_help_and_invalid_options_need_no_download_and_old_python_is_rejected` (old-Python half replaced by "no interpreter needed")
- `tests/contracts/test_download_diagnostics.py::DownloadBootstrap.test_invalid_signature_is_not_retried`
- `tests/contracts/test_download_diagnostics.py::DownloadBootstrap.test_transient_manifest_failure_recovers_to_authenticated_install` (first asset is now the binary)
- `tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_recovery_discards_partial_bytes`
- `tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_no_retry_for_permanent_errors`
- `tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_resource_signal_not_retried`
- `tests/contracts/test_download_diagnostics.py::ReviewRegressions.test_http_permanent_status_wins_over_transient_curl_exit`

Partially ported (bootstrap half here; install/receipt/update half waits for the Rust installer/CLI `am install`):
- `tests/contracts/test_installer.py::OneLineBootstrapContract.test_default_one_line_install_ignores_caller_checkout_and_keeps_runtime_after_cleanup`
- `tests/contracts/test_installer.py::OneLineBootstrapContract.test_standalone_copy_forwards_equals_options_and_checkout_entry_still_works`
- `tests/contracts/test_installer.py::OneLineBootstrapContract.test_clone_and_install_failures_propagate_and_clean_scratch` (clone moved into the binary)
- `tests/contracts/test_installer.py::MacOSCommandContract.test_aliased_tmpdir_pipe_preview_install_update_and_both_commands`
- `tests/contracts/test_download_diagnostics.py::DownloadBootstrap.test_failed_download_never_activates_and_has_guidance` (private diagnostic removed at this stage by design)
- `tests/contracts/test_download_diagnostics.py::DownloadDiagnostics.test_http_retry_and_deadline` (retry ported; shared 30 s budget is a declared delta)

Deferred to the binary's `download-failure-v1` implementation (Rust `release_distribution`/installation, not the shell): `DownloadDiagnostics.test_private_bounded_redacted_evidence`, `DownloadDiagnostics.test_retention_failure_preserves_primary_error`, `ReviewRegressions.test_diagnostic_redaction_normalizes_before_matching`, `ReviewRegressions.test_report_policy_failure_preserves_primary_error`, `BootstrapPreview.test_signed_preview_uses_packaged_guidance_and_preserves_profiles`, `PreviewGuidance.test_preview_is_not_installation_and_does_not_request_restart`; `OneLineBootstrapContract.test_pipe_preview_repeat_update_and_uninstall_preserve_memory_and_user_instructions` (managed remote channel journey, binary-side).
Obsolete with Python removal: `InstallerContract.test_wrapper_chooses_supported_python_after_older_path_entry`; the `test_gate_diagnostics.py` checks that read `.github/workflows/contracts.yml` describe the Python gate.

## Declared deltas
- Child run, not `exec` (agreed with Delivery): `exec` would discard the EXIT trap and leave the downloaded binary in TMPDIR. The binary must not keep a reference to the path after exiting.
- Deadline: POSIX sh has no monotonic clock, so the baseline's shared 30 s budget becomes per-attempt `--max-time 60` (binary is up to 64 MiB) and `--connect-timeout 5`; retry delays 1 s and 2 s (integer `sleep`) instead of 0.25/0.5 s. Worst case ≈ 3×60+3 s per asset. Reason `deadline` (Python timeout) no longer occurs; curl 28 `transfer timeout` covers it.
- No private diagnostic file and no curl stderr detail at this stage (design). Failure JSON always on stdout, independent of `--json`.
- `ulimit -f` uses 512-byte blocks in dash/POSIX mode but 1024-byte units in non-POSIX bash, so the rlimit may allow up to twice the limit; the exact limit is still enforced by `--max-filesize` and the final `wc -c` check.
- Both assets come from `releases/latest`; if `latest` moves between the two fetches, verification fails safe (`invalid-signature`); retrying converges.
- A non-executable TMPDIR (noexec mount) surfaces as the shell's exit 126 without JSON, because it cannot be distinguished from the binary's own exit status.
- A truncated prefix holding only the leading comments runs nothing and exits 0 (any prefix reaching the program is a syntax error).

## Workflow (`.github/workflows/contracts.yml`)
Trigger kept as `workflow_dispatch`; `permissions: contents: read`; checkout `3d3c42e5…` (v7.0.1, `persist-credentials: false`) and upload-artifact `cf430e03…` (v7.0.2) pinned as before; setup-python and download-artifact gone. Jobs: `test` on `macos-15` (aarch64-apple-darwin), `macos-15-intel` (x86_64-apple-darwin), `ubuntu-24.04` (x86_64 Linux), `ubuntu-24.04-arm` (aarch64 Linux): rustup stable + rustfmt, host-triple assertion, `cargo fmt --check`, `cargo test --locked`. `musl` on `ubuntu-24.04`/`ubuntu-24.04-arm` (native linker per arch): `cargo build --locked --release --bins --target {x86_64,aarch64}-unknown-linux-musl`, requires `target/<triple>/release/am` to exist and be statically linked (`file`), retains it as an unsigned 14-day artifact. `gate` requires both. No release publication. Notes: this job fails until the CLI lane adds the `am` binary target; toolchain is rustup `stable` (no `rust-toolchain.toml` in the tree); macOS runners have no `dash`, so tests there cover `/bin/sh` and `bash`. Checked with `actionlint` (clean) and a YAML parse.

## Commands run (worktree, macOS arm64)
- `sh -n install.sh`, `dash -n install.sh`, `shellcheck install.sh`, `shellcheck -s dash install.sh` → clean.
- `cargo fmt` then `cargo fmt --check` → clean.
- `cargo test --offline --test t2_bootstrap` ×3 → `26 passed; 0 failed` each (≈4.6 s).
- `cargo check --offline --all-targets` → no warnings in my files (only the known `t1_a2` warnings).
- `actionlint .github/workflows/contracts.yml` → clean.
- Process note: one accidental `python3 -` with empty stdin was run while grepping (no program, nothing inspected); it informed nothing here.
