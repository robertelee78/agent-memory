# Independent review: installation lane at `fabc937`

Scope: committed `fabc937` in `/opt/agent-memory-worktrees/rust-v015-int` (read with `git show`/`git archive`; the worktree's uncommitted edits were ignored). I read in full: `src/installation.rs`, `src/installation/{adoption,filesystem,lock,plan,providers,records,seams,transaction}.rs`, `src/runtime_faults.rs`, `tests/t2_installation.rs`, `tests/fixtures/v014-state/derive.sh` and the fixture layout, plus the installation-related diffs in `src/console.rs`, `src/cache_recovery.rs`, `src/omp_installation.rs`, `src/artifact.rs`, `src/fs/descriptor.rs`, `src/cli/{mod,table,wiring}.rs`, `src/cli/helpers/installation.rs`, `tests/t2_cli.rs`, `tests/t2_delivery.rs` and `tests/wiring-pending.txt`. Baseline: `installation.py` (all 1,604 lines), `runtime_faults.py`, `omp_installation.extension_wrapper`, `tests/lifecycle_fault_fixtures.py` (the driver behind every `test_install_write_*`). Design: the sections named in the assignment. No cargo, no Python, no Git writes.

## Verdict

**No P1 defect and no P2 code defect that blocks merge.** Every lifecycle write I traced is journaled and recoverable. Ownership is re-verified before every replace. The Python-era recognisers are exact and fail closed on a near-miss. The bootstrap path checks the bootstrap binary against the signed manifest and installs exactly those bytes. Runtime faults cannot be triggered from the ambient environment, and the lock/owner protocol works alongside a live Python owner.

The findings are one P2 design deviation in the bootstrap's release URL and several P3 gaps. The most important P3 is the test gap: the release runtime-root path, the bootstrap binding and the release-mode adoption are never run by any test. An offline signed fixture already exists in `t2_delivery`, so that is fixable now. The "every install write interrupted" claim covers only a fresh development-source install, not the write set of the Python baseline it ports.

Production note (not a defect; already declared as pending): with `updating`, `private_state` and `omp_boundary` unregistered, `am install` refuses before any write (`Error::Unregistered`). An uncommitted journal cannot be rolled back by `am recover` in production either, because `rollback` needs `private_state` and `updating`. Adoption therefore only becomes reachable once those three registrations land; the release gate must include the hook route (finding 7).

Overall correctness: **correct** (no merge blockers). Confidence: 0.75.

## Findings

### F1 (P2, confidence 0.6): Bootstrap fetches the signed manifest from `releases/latest`, not the bootstrap's own tag

- Rust: `src/installation/seams.rs:112-116`. `ReleaseAccess::default().first_manifest_url` = `https://github.com/{REPOSITORY}/releases/latest/download/release-manifest.json`. `stage_release` (`plan.rs:833-861`) probes that URL and only afterwards requires `manifest.version == CARGO_PKG_VERSION` (`release_distribution::bind_bootstrap_binary`).
- Design "Binding the bootstrap binary to its release": "Version-pinned URLs (tag, not `latest`) are used for every asset after the first." The first asset is the binary that `install.sh` fetched (`install.sh:12,311`). The manifest is the second asset and the binary knows its own version.
- Failing interleaving: `install.sh` downloads `am-<triple>` while `latest` = v0.0.15. v0.0.16 is published. The binary then fetches the `latest` manifest (v0.0.16) and refuses "Bootstrap binary version differs from its signed release; re-run the trusted bootstrap". The same refusal hits any operator who pins a specific release binary. The check fails closed, so this is a wrong refusal, not a bypass. Python had no binary to bind, so there is no baseline line.
- Minimal fix: derive the first URL from the compiled version: `first_manifest_url: format!("{}/v{}/{}", release::ASSETS, env!("CARGO_PKG_VERSION"), release::MANIFEST)`. `probe_with` already requires `tag == "v{version}"`.

### F2 (P3, confidence 0.85): The release runtime root, the bootstrap binding and release-mode adoption are untested, although an offline signed fixture exists

- Every install and adoption test uses `--development-source` (`tests/t2_installation.rs:308-313`, `adopt` at `:2029`). No test therefore covers:
  - the `source` action that copies, backs up and exchanges `<data>/agent-memory/runtime`;
  - `source_info(kind="release")`;
  - `stage_release`'s `bind_bootstrap_binary` and the "same bytes" guarantee;
  - the legacy-to-release path the design actually ships.
- `interrupted_adoption_rolls_back_before_commit_and_never_after` excludes `data/agent-memory/runtime` from its comparison (`:2548`), but a development-source adoption never creates that directory. The exclusion is vacuous and the runtime-root rollback is never observed.
- The author report (line 82) says a real signed run is release qualification. But `tests/t2_delivery.rs:470-505` already drives `release::probe_with` offline with `file://` URLs and a test `Publisher` key. `Services.release` (`ReleaseAccess { first_manifest_url, assets, options, signers, version }`) is injectable, so the whole bootstrap path can run in the default suite with no network.
- Minimal fix: add a release scenario that publishes a signed manifest, archive and per-target binary. Point `ReleaseAccess` at `file://` and run:
  1. a fresh `--bootstrap-binary` install, asserting `runtime/bin/am` bytes == bootstrap bytes and `receipt.artifact_files["bin/am"]`;
  2. a tampered bootstrap binary, asserting refusal before any write;
  3. a v1 adoption through the bootstrap;
  4. the partition replay below in release mode.

### F3 (P3, confidence 0.8): The partition replay ("every observed install write") does not cover the baseline write set it is marked as porting

- Rust: `tests/t2_installation.rs:1600-1755` replays only a fresh development-source install with `--presets codex`.
- Baseline `tests/lifecycle_fault_fixtures.py:36,126` replays an install whose source is the managed checkout copied into `code`. It requires `payload-snapshot` for both modes (the `source` directory node). It also asserts `physical_targets_match_clean`, i.e. per-target node snapshots, and a doctor health check.
- In Rust's link mode no directory payload is written at all; the test asserts `payload-snapshot` only when `install_mode == "copy"` (`:1646`). The directory-node stage/backup/exchange of the `source` kind, the main write of a Rust release install, is never interrupted. Neither is any write of an adoption (wrapper-file to command-link exchange, hook-config rewrite of an existing file) except the two receipt faults in `interrupted_adoption…`.
- The 32 `test_install_write_*` obligations are recorded as fully ported.
- Minimal fix: run `replay_install` in the release mode from F2, so `source`/`payload-snapshot` occur in both modes. Restore the `payload-snapshot` assertion for link mode. Compare per-target `fingerprint`s after retry, not only receipt semantics. Add one adoption replay, or mark these obligations `partial` until then.

### F4 (P3, confidence 0.7): Adoption fixtures overwrite every captured Python identity with Rust's own fingerprint

- Rust: `tests/t2_installation.rs:1954-2003` (`materialize`) replaces every target's `identity` with `install::fingerprint(&path)`. `store_receipt` then recomputes the journal's `receipt_identity`.
- For relocated bytes and opaque trees this is unavoidable. But it also replaces identities that need no relocation, such as `codex/AGENTS.md`, `claude/CLAUDE.md` and `config/opencode/AGENTS.md`, which contain no `@WORLD@`.
- So no adoption test checks Rust's `fingerprint` against a genuine v0.0.14 identity. The design row requires "a regression against captured v0.0.14 state", and that row's ownership check stands on exactly this equality. A mismatch, for example in mode encoding or key layout, would make every real adoption refuse "Unrelated or user-modified target" while the suite stays green.
- Minimal fix: keep the captured identity for every file whose bytes `derive.sh` did not rewrite and assert `fingerprint == captured`. Optionally keep one tiny captured directory tree to pin the directory hash.

### F5 (P3, confidence 0.7): The bootstrap stages (downloads) the release before refusals that need no release

- Rust: `src/installation/transaction.rs:1625-1628` always calls `stage_release` when `--bootstrap-binary` is given. The `--remote/--ref` refusal comes later in `prepare` (`plan.rs:1248-1263`, `MANAGED_GUIDANCE`).
- Baseline `installation.py:1551-1555` gates the acquisition on `default_release` (no `--remote/--ref`, no development source, compatible receipt).
- Failing input: `curl … | sh -s -- --remote https://example/x.git`. The binary downloads the archive first. With the network down, or a download that exceeds its bound, the user gets the `download-failure-v1` repair instead of the managed-channel guidance the operator decision requires. Online, a 16 MiB archive is fetched only to refuse.
- Minimal fix: before `stage_release`, refuse `request.remote/reference` unless `old` is a legacy managed receipt on the same channel, mirroring `prepare`'s rule. Skip staging when `old` is a Rust-era development receipt, which `prepare` ignores anyway.

### F6 (P3, confidence 0.6): The per-action rollback compatibility re-check skips `command-link`, the Rust replacement of `wrapper`

- Rust: `src/installation/transaction.rs:496-503` (`EXECUTABLE_KINDS.iter().take(4)` = source/copy/link/wrapper).
- Baseline `installation.py:1085` re-runs `rollback_compatibility` before restoring each executable entry point. In v0.0.14 that entry point is the `wrapper`; in Rust it is `command-link` (`bin_dir/am`, `bin_dir/agent-memory`).
- Interleaving: an interrupted adoption rolls back. The initial `rollback_compatibility` passes. A bank is then migrated by a concurrent writer before the `bin_dir/am` step. The Python wrapper is re-activated without the re-check the baseline performs for exactly this case. The initial check bounds the impact.
- Minimal fix: include `command-link` in the per-action set, for example `matches!(kind, "source" | "copy" | "link" | "wrapper" | "command-link")`.

### F7 (P3, confidence 0.7): Hook commands target an `am hook` route that does not exist, with no guard

- Rust: `src/installation/plan.rs:420-436` writes `<am> hook codex-prompt|codex-stop|turn-hook|claude-stop` into Claude `settings.json` and Codex `hooks.json`. Adoption rewrites the Python commands to these strings.
- Consumer: `src/cli/table.rs` at `fabc937` has no `hook` entry; no `"hook"` route exists anywhere in `src/`. `tests/wiring.rs` guards only `register_*` functions.
- Today this is masked because install cannot run in production (see the production note). Once the three seams are registered, an adoption would leave every client prompt/Stop hook invoking an unknown subcommand.
- Minimal fix: add a test that parses each `client_hook_entries` command through `cli::dispatch` (or the console parser), or list `installation hook commands wait for cli hook route` in `tests/wiring-pending.txt` so the release gate sees it.

### F8 (P3, confidence 0.45): The repeated-bootstrap rewrite to `update` drops the verified `--bootstrap-binary`

- Rust: `src/console.rs:275-276,326-333`. When a v3 release receipt exists, `install --bootstrap-binary X …` becomes `["update"]` (or `["update","--check"]`) and `X` is discarded.
- The design's binding rule ("installs those same bytes; never downloads the binary again") then cannot be applied by the update that runs. Separately, `update` is not routed at `fabc937`, so the repeated one-liner fails until `updating` lands.
- Minimal fix: carry the path, e.g. `["update", "--bootstrap-binary", X]`, and declare that option for `updating`. Alternatively, record in the updating hand-off that `update` binds `current_exe()` to `targets[TARGET]`.

### F9 (P3, confidence 0.5): `verify_targets` drops the client-visible artifact readback for Python-era sources

- Rust: `src/installation/transaction.rs:210-221` runs the copy/link readback only when `source.target` exists.
- Baseline `installation.py:975-978` always checks `artifact.inspect(path)['artifact_digest'] == target['artifact_digest']`.
- Reachable path: the design's out-of-band recovery ("run the verified bootstrap binary with `recover`") on a v0.0.14 journal that is `complete`, or `activating` with a committed receipt. For a Python-era development install, link targets point at an unowned development pack. A drifted pack is reported `verified: true, status: complete`, and the `activating` journal is rewritten to `complete`, where the baseline refuses "Client-visible artifact readback failed". For managed/release sources the `source` target's directory hash covers the same bytes, so the impact is limited to development sources.
- Minimal fix: add an `else` branch for non-Rust-era sources that compares `services.artifacts.inspect(&path)` with `target.artifact_digest`.

## Weak tests (beyond F2–F4)

- `adoption_is_busy_while_a_python_owner_holds_the_lock` holds the `flock` from the test process itself. That is equivalent at the kernel level, since `flock` is per open file description. Python's own open flags (`O_RDWR|O_NOFOLLOW`, no `O_NONBLOCK`) are not exercised. Acceptable.
- `adoption_rolls_back_an_in_flight_python_journal_before_starting` covers only an `activation` step. No in-flight `wrapper`, `copy` or `source` step is rolled back from a Python journal, and those are the kinds whose restore needs `rollback_compatibility`.
- No test covers a Python-era `release`-kind receipt (all four captures are managed or development) or the OpenCode `hook-link` near-miss.

## Focus areas checked and found correct

- **Every write journaled/recoverable:** I traced each write. Each one below is a numbered `runtime_faults::mutate` step at the baseline position, and recovery covers every interruption point:
  - `make_state_directory`, the lock seed (`file-stage`, `atomic-node-switch`, `stage-cleanup`) and the owner `replace_record`;
  - the journal history copy and the journal publication;
  - `make_parents`, which journals the directory before `mkdir`;
  - link/copy/file staging, backup snapshots, activation exchanges and the receipt backup/publication;
  - rollback renames, journal status, and the preserved pre-exchange history.

  Recovery handles each case: an unchanged target is skipped, a target equal to `after` with the stage equal to `before` is exchanged back, a fresh target is renamed to its stage, and a committed receipt is verified forward. Probe directories are excluded exactly as in the baseline (`temporary_probe`).
- **Ownership before replace/delete:** `add()` refuses any existing node without an identical receipt identity. Activation re-checks `path == before && stage == after` immediately before the exchange and reads back after it. Rollback exchanges only when `current == after` and `stage == before`; otherwise it records a conflict. Nothing is unlinked except the installer's own uniquely named `.record-*`/`.lock-seed-*` stages and its probe tree. `Scratch` cleanup is limited to the private 0700 `/tmp` directory it created.
- **Python-era recognisers:**
  - The wrapper bytes are byte-identical to `installation.py:wrapper`, checked against all four captures. The receipt identity must equal the exact wrapper (`0o755`) or the source-only launcher, and the on-disk node must equal that identity. A near-miss in either refuses as foreign (tested both ways).
  - Hook commands are rebuilt as `'/bin/sh ' + quote(pack/hooks/python3.sh) + ' ' + quote(pack/hooks/<script>)`, with `asyncRewake` for Claude Stop. Comparison is structural and type-strict, so a `true`/`1` near-miss refuses where Python's `==` would accept (fail closed). Only the receipt-located groups are merged.
  - The activation block hash is verified; a modified block refuses.
  - The OMP extension wrapper matches `omp_installation.extension_wrapper` (JSON-encoded source, compact options, `rolloutAttested: true`).
  - Non-plain paths fall back to "foreign", a declared delta.
- **Bootstrap binding:**
  1. `stage_release` refuses a missing or non-ordinary bootstrap binary before any download.
  2. It verifies the manifest signature and tag, then requires the compiled version and the target sha256/size (`bind_bootstrap_binary`). The bytes it returns are the bytes written as `bin/am` with mode 0755.
  3. Under the lock, `source_info(release)` re-verifies through `installed_with`: signature, pack identity, `bin/am` sha/size/mode/nlink, per-target runtime digest and exact layout. It also compares `inspect_runtime(...)` with `targets[TARGET].artifact_digest`.
  4. The staged directory is installed only if `fingerprint(stage) == after`, where `after` is computed from that verified scratch.

  Only F1 deviates.
- **runtime_faults:** `operation` arms only if `AGENT_MEMORY_FAULT_STAGE|TRACE|ERROR_STAGE` is non-empty, and then requires a validated World: direct child of canonical `/tmp`, named `agent-memory-test-*`, not a link, marker token equal to `AGENT_MEMORY_TEST_TOKEN`. Every lifecycle path must be strictly inside that World and the trace inside it with no `..`; otherwise it refuses before any write. Direct `bank_writes::fault` points validate the World independently. Probe writes are not numbered.
- **Symlink/permission races on bin_dir and client configs:** every create, rename and fsync goes through a pinned `DirectoryChain` with a re-check. `open_raw_at` always adds `O_NOFOLLOW|O_CLOEXEC`. `unchanged_parent` and `trust::tree` run before each activation. A concurrent client edit between check and exchange is caught by the post-exchange readback and preserved as a rollback conflict, as in the baseline.
- **Command-link atomicity for a running `am`:**
  - Adoption swaps the wrapper file and the link in one `renamex_np(RENAME_SWAP)`/`renameat2(RENAME_EXCHANGE)`.
  - Rust-to-Rust updates exchange the whole runtime directory. The link text `<runtime>/bin/am` never changes, and a running process keeps its old inode under the stage name.
  - Nothing is written in place into an executing file, which also matters for macOS code-signing vnode caching.
- **Receipt v3 field rules:**
  - The receipt sets `schema` v3 and `source.target` = compiled triple, for release and development alike.
  - `source.artifact_digest` is the per-target runtime digest. `artifact_files` = `inspect_runtime(pack, binary).files`, which includes `bin/am`.
  - Target `artifact_digest` is the runtime digest for copy/link/hook/extension. `legacy-source` carries `legacy_source`.
  - `installation_id`, `backups` and `created_directories` are inherited from the Python receipt. The prior receipt is backed up to `receipt-<tx>.json`.
  - `omp_installation::validate_receipt` requires `source.target` for v3 and allows v3 with or without a binding. `cache_recovery` binds the `bin/am` entry and both digests (new `t2_delivery` test).
- **Lock/owner.json with a live Python owner:**
  - It uses the same `state/lock` inode (seeded only by no-replace rename, never rewritten).
  - It takes a non-blocking `flock(LOCK_EX)`, so a live Python owner yields the busy refusal before owner metadata is touched.
  - The lock file must be a regular single-link file with the lock schema. `owner.json` must have the owner schema before it is atomically replaced with the same `{schema, lock_path, pid, started_at}` and the same `json.dumps(sort_keys, indent=2)` bytes.
  - A dead owner leaves no flock, so the existing recovery rule applies.
- **Journal step kinds:** `read_journal` refuses any unknown kind before any write, naming the retained evidence. A Python v1 in-flight journal is finished or rolled back under the lock before adoption plans.
- **macOS vs Linux:** no platform-specific logic was added in installation beyond the foundation's rename/flock wrappers and the `TARGET` cfg. `TARGET` still names `x86_64-apple-darwin` and two `-gnu` triples. Those are harmless, because the manifest lookup refuses an unpublished triple, but they could be trimmed to the three supported targets.
