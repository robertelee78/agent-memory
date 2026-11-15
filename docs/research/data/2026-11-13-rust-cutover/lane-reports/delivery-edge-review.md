# Delivery-edge review: bootstrap, OMP installation, migration catalog

Reviewed the committed tree at `/opt/agent-memory-worktrees/rust-v015-int` (branch `migrate/rust-only-v0.0.15`, HEAD `c3e76a3`). The reviewed files are unchanged since their lane commits `1560d6c`, `7622e91`, `4de6a76` and `5daf7d3`. Uncommitted work by other agents is in the worktree, but none of it touches these files. The baseline is the Python under `skills/agent-memory/scripts/agent_memory/` plus `git show 1560d6c^:install.sh`. Nothing was edited, built or executed; only read-only `git log/show/diff` was used.

## Verdict

**Incorrect: not mergeable as-is.** The shell bootstrap is sound. I found no way to run anything before signature verification, and no injection, quoting or namespace-replay defect. The digest definition and the migration-graph logic match their specifications. Three P2 defects remain:

1. The OMP configuration requires `--omp-peers-upgraded` on every re-issue. That breaks baseline `am update` and `am doctor --fix` on every OMP install.
2. The private OMP receipt reader drops baseline `read_receipt` gates (the 1 MiB bound, `verified`, the target shape, `uninstalled`).
3. The production `am` never registers the migration provider, so `am artifact inspect` and every `inspect_runtime` caller always refuse.

There are also four P3 items. Confidence in the verdict: 0.85.

---

## Findings

### P2-1: Re-issuing a saved OMP binding without `--omp-peers-upgraded` refuses, which breaks `am update` and `am doctor --fix`
- **Where:** `src/omp_installation.rs:160-175` (`configuration`, the "A Rust re-issue never inherits…" branch).
- **Baseline:** `omp_installation.py:41-47` returns `binding(previous['agent_dir'], …)` without the flag when the saved capability is already `omp-stored-source-v2`.
- **Callers that depend on this:**
  - `updating.py:26-29,333` passes the user's flag (default `False`) for every `am update`. That includes the "unchanged" update, because `prepare` always calls `configuration`.
  - `doctor_fix.py:41` uses the hard-coded `updating.configuration(old)` (flag `False`). `doctor --fix` has no way to supply the flag.
- **Failing input:** any receipt-v2 OMP install whose binding is current, then `am --json update` or `am doctor --fix`.
  - Baseline: proceeds. The obligation `tests/contracts/test_omp_boundary.py::…test_managed_native_updates_read_back_only_owned_public_commands` (lines 1316-1333) runs `update` with no flag and expects exit 0.
  - Rust: `Native omp requires --presets omp, --omp-agent-dir and --omp-peers-upgraded …`.
- **Why it matters:**
  - The design only says "never inherited" for the **Python-era adoption** row ("OMP binding and extension target … `rollout_attested` must be supplied again explicitly"). It does not say this for Rust-to-Rust re-issue.
  - The lane report declares the broader delta but not its consequences. Signed-release `am update` is the only update channel, so it must be re-attested on every run, and `doctor --fix` hook reconciliation can never succeed for an OMP install. That obligation cannot be ported.
- **Minimal fix:** require the renewed flag only when adopting a Python-era receipt, or when the saved capability is not current. Otherwise return `binding(root, no_guidance, READER_CAPABILITY)` as the baseline does.
  - Use a deterministic discriminator: a Python-era receipt's `artifact_files` has no `bin/am` entry (`artifact::BINARY_ENTRY`). Alternatively, the installation lane can pass an explicit `adopting: bool`.
  - Change `rust_reissue_never_inherits_current_rollout_attestation` to assert both arms.
- **Confidence:** 0.85

### P2-2: The private OMP receipt reader drops the baseline's receipt gates and uses a 16× larger bound
- **Where:** `src/omp_installation.rs:13` (`MAX_RECEIPT` = 16 MiB) and `:529-547` (`read_receipt`). Callers are `observer_binding_with` (`:556`) and `verified_observer_binding` (`:622`).
- **Baseline:** `installation.read_receipt`, `installation.py:384-402`, with `load` at `:101-105`. It does all of the following before `validate_receipt`:
  - `size <= 2**20`, not a link, else `Missing, linked or oversized installation record: PATH`.
  - `verified is True`, `installation_id` is a str, `targets` is a list and `source` is a dict, else `Invalid installation ownership receipt: PATH`.
  - Each target is a dict with a kind in {copy, link, wrapper, activation, source, hook-config, hook-link}, a normalized absolute `path` and an `identity` key, else `Malformed target ownership receipt`.
  - `uninstalled is True` returns `None`.
- **Failing input:** the receipt written by baseline uninstall (`installation.py:1308-1325`). It keeps `omp_binding` and sets `uninstalled: true` and `artifact_files: {}`.
  - Baseline `observer_binding`: `Native observer is not the current explicit owned binding`.
  - Rust: accepts the receipt, runs `validate_receipt`, `verify_targets` and a full 64 MiB `inspect_runtime` against uninstalled targets, and refuses later with a different string. At the latest that is `Native observer executable differs from installed artifact`, and only because `artifact_files` happens to be `{}`.
  - A 1.5 MiB receipt, or one with `verified: false`, is accepted where the baseline refused.
- **Why it matters:** the report's declared delta ("former unbounded receipt … reads") is factually wrong. The baseline read was bounded at 1 MiB.
- **Minimal fix:** port `read_receipt` exactly, with the 1 MiB bound, both gates with the baseline strings, and `uninstalled` → `Ok(None)`. Better still, call the single installation receipt reader when it lands rather than keeping a second reader here.
- **Confidence:** 0.8

### P2-3: The production `am` never registers `migration_contract::Bundled`, so artifact inspection always refuses
- **Where:** `src/cli/mod.rs:269-272` (`register_providers`). This is the consumer side of the provider added in `src/migration_contract.rs` ("The production implementation of artifact inspection's migration seam").
- **Baseline:** `artifact.py:75-76` always imports `migration_contract`.
- **The dispatch path:** `am artifact inspect` → `cli/helpers/artifact.rs:38` → `artifact::command` → `inspect` → `inspect_validation`. That last step refuses when the registry is `None` (`artifact.rs`, "Migration contract provider is not registered; artifact inspection refuses").
  - Only `src/bin/am_dev/main.rs:19` (release builder) registers `Bundled`.
- **Failing input:** `am artifact inspect`.
  - Baseline: `valid: true`.
  - Rust: `valid: false` with that error, exit 1.
  - The same refusal hits every `inspect_runtime` caller, including the OMP observer (`omp_installation.rs:572`) and the install/doctor paths when they land.
- **Minimal fix:** in `register_providers`, also call `crate::artifact::register_migration_contract(Some(Arc::new(crate::migration_contract::Bundled)))`, mapping its `ContractError` into `Invalid`.
- **Test gap:** `tests/t2_cli.rs` has no successful `am artifact inspect` case.
- **Confidence:** 0.85

### P3-1: Catalog v2 removes the per-entry code binding, but nothing yet guarantees that its replacement (F) is what migration plans bind to
- **Where:** `src/migration_contract.rs:1-8` (doc claim "the artifact digest already binds") and the `entry` shape.
- **What the baseline relied on (search result):** `migration.py:158-166` `invoke` re-hashes `entry['module']` against `entry['sha256']` before import. `migration.py:356-366` `check_code` binds a plan to `catalog['sha256']` and to `artifact.inspect(PACK)['artifact_digest']`. That is the pack-only digest, which covered the `migration_*.py` code. No other baseline consumer reads entry `sha256`/`module`; `recall_catalog.py:39` lists module names for its bytecode identity, which design item 5 replaces.
- **What changes in Rust:** codec code lives in the binary, which only the runtime digest F (`inspect_runtime`) covers. The pack-only `artifact::inspect` still returns `migration_catalog`, and the design's "Artifact digest" list (receipt targets, OMP binding, same-version identity, doctor) does not name migration plans.
- **Consequence:** a T2b port of `catalog()` written literally as `artifact::inspect(PACK)` would let a plan prepared under binary A resume under binary B with different converters and an identical pack.
- **Fix:** add migration plan identity (`artifact_digest` in `check_code`/plan) to the F list and the module contract. Alternatively, have `migration.rs` use `inspect_runtime` only.
- **Not a defect:** Python-era in-flight plans (`catalog_sha256` of the v1 bytes) refuse with `Migration runtime/catalog changed; retain the original plan…`. That is the design's explicit recovery requirement.
- **Confidence:** 0.6

### P3-2: Non-finite catalog refusal string differs
- **Where:** `src/migration_contract.rs:228`.
- **Baseline:** `migration_contract.py:61` (`parse_constant` → `Nonfinite migration catalog`).
- **Failing input:** a catalog whose bytes match the inventory sha but contain `NaN` (e.g. an extra `"x":NaN`). Rust returns the codec's `Non-finite JSON value`. Duplicate keys already match (`Duplicate metadata key: K`).
- **Fix:** decode with `NonFinite::Accept`, then refuse any non-finite `Float` with the baseline string. Or map the decoder's non-finite error at this call site.
- **Confidence:** 0.7

### P3-3: `target()` picks the last duplicate entry where the baseline picks the first
- **Where:** `src/omp_installation.rs:191-197` (`.rev().find`), used by `observer_binding_with:566` and `answer_first_qualification:410`.
- **Baseline:** `omp_installation.py:183` (`owned = next(…)`) and `:115` (`next(…)`) select the first match. Only `validate_receipt`'s dict (`:74`) takes the last.
- **Failing input:** a receipt listing the extension path twice. Rust verifies a different owned record than the baseline.
- **Fix:** use a first-match helper for the observer and answer-first callers, and keep last-match only in `validate_receipt`.
- **Confidence:** 0.4

### P3-4: A signal during `mktemp` can leak an empty 0700 scratch directory
- **Where:** `install.sh:137`.
- **Trigger:** a SIGTERM to the shell, or SIGHUP, arriving while `am_directory=$(mktemp -d …)` is still being evaluated. The trap runs `exit 143` before the assignment, so the EXIT trap sees an empty `am_directory` and leaves `$TMPDIR/am-bootstrap.XXXXXXXX`.
- **Fix:** run `trap '' HUP INT TERM` immediately before the `mktemp` assignment and reinstate the three traps right after it.
- **Confidence:** 0.4

---

## Checked and found correct

### (1) `install.sh` / `tests/t2_bootstrap.rs`
- **Nothing runs before verification.** The program is a single `if :; then … fi`, so a truncated pipe is a syntax error. Help handling, `uname`, option validation, `command -v`, `mktemp`, `curl` and `ssh-keygen` all run before the binary does. The binary runs only after `am_verify` returns 0 under `set -eu`.
- **Children cannot read the script.** `curl | sh` children get EOF on stdin because the whole compound command is parsed first.
- **Quoting and option injection:**
  - Every expansion is quoted.
  - `curl … -- "$am_url"` takes `--disable` first.
  - `rm -f --` / `rm -rf --` are used.
  - The `mktemp` template and every `chmod`/`ssh-keygen` path start with `/`, because TMPDIR must be absolute or falls back to `/tmp`.
  - User arguments reach only `am`, after `install --bootstrap-binary PATH`.
  - `--bootstrap-binary` from the user is `unknown-option`.
  - Option syntax matches baseline `bootstrap()` (`install.sh@1560d6c^:454-473`).
- **TMPDIR and the private directory:** `mktemp -d` creates the directory atomically at 0700. The binary is verified and run from that same-uid 0700 directory, so another user cannot swap it between verify and exec.
- **Exit paths:** every `am_fail`/`exit`, every `set -e` abort and HUP/INT/TERM pass through the EXIT trap. The only exception is P3-4.
- **Truncated downloads:** partial bytes are removed before each attempt. Size is enforced three ways: `--max-filesize`, `ulimit -f` in the curl-only subshell (not inherited by `am`) and `wc -c`. The signature check rejects any truncation curl reports as success (tested: half, empty, appended).
- **Failure JSON:** every string comes from case tables. The only integers are `$?` and an HTTP code constrained to `[1-5][0-9][0-9]`, and `uname` bytes are never echoed (tested with an injected kernel name).
  - Small parity note: baseline `[0-9]{3}` and not `000` also admits 6xx-9xx statuses, which affects only the retry decision for such codes. This is negligible.
- **Namespaces and signers:** the pinned allowed-signers line permits both namespaces for one key. SSHSIG binds the namespace into the signed data, and `-n agent-memory.release-binary` is passed explicitly, so a manifest signature cannot verify as a binary signature (tested). Rust verifies manifests with `-n agent-memory.release` (`release_distribution.rs:487`), so the reverse replay also fails.
- **`latest` used for both assets:** if `latest` moves between the two fetches, verification fails safe. The design's "tag-pinned after the first asset" is not met for the `.sshsig`, but this is declared and has no security effect.
- **dash, bash and macOS `sh`:** the only behavioural difference is the `ulimit -f` unit (declared, and backstopped by `wc -c`). zsh-as-`sh` also uses 512-byte units.
- **Could the tests pass with verification removed?** No:
  - Removing `am_verify` or making it pass fails three tests (another key, manifest namespace, truncated/altered).
  - Switching to `ssh-keygen -Y check-novalidate` fails the another-key test.
  - Changing the namespace fails the happy-path test.

### (2) `src/omp_installation.rs` / `src/artifact.rs`
- **`validate_receipt`:** order, messages and duplicate (last-wins) semantics match.
- **`binding`:** shape and order match; the spelling normalization is declared.
- **Wrapper bytes:** the import specifier uses ensure-ASCII JSON and the options are compact ASCII, as in the baseline. The `executable` field is added second, as agreed with T2f. The current `omp.ts` ignores unknown option keys, so the old adapter does not break on load.
- **`rollout_attested`:** `binding()` always emits `true`, and nothing in this module copies it from a receipt. P2-1 is the opposite problem (over-requiring the flag).
- **Single activation interface:** one `RwLock<Option<Arc<dyn OmpActivation>>>`. Absence is the typed `MissingActivation`. Proofs capture the provider at mint.
- **Observer proof:** private fields, no `Clone`, no serialization. The receipt sha, fingerprint and location are rechecked inside the provider's barrier/marker scope before the callback, matching baseline `_observe_observer_binding` (`:258-282`).
- **Digests:**
  - There is one definition: `runtime_digest` (sorted compact ASCII, as baseline `inventory_digest`). `release_distribution` and `cache_recovery` reuse it.
  - A pack that already has any `bin/` entry is refused.
  - The binary must be exactly mode 0o755 and is read through a pinned no-follow descriptor, bounded at 64 MiB, inside the pack's validation bracket.
  - The observer compares both F and the complete `artifact_files` inventory against `inspect_runtime`.
- **Manual comparison:** directory pins (dev, ino, mode, uid, gid), the full file pin, the link pin, the 16 MiB bound and the `0o…` mode string all match `_manual_comparison`.

### (3) `src/migration_contract.rs` / `src/migration_codecs.rs` / `migrations.json`
- **Shape checks:** all messages, the 1..64 codec and ≤64 step bounds, and the regexes (`[a-z][a-z0-9_]{0,63}`, `agent-memory/[A-Za-z0-9_.-]{1,128}`, `[a-z0-9][a-z0-9_.-]{0,127}`) are exact.
- **The `visit` memo (`route`):** keeps the baseline order (cycle check, then target check, then memo lookup). The memo is fresh per call and the ambiguity check is the same.
- **`acyclic`:** shares `complete` across roots in declaration order, like the baseline.
- **Advertised migration sources:** `supported` short-circuits per target like `any()`, then every (source, readable) pair is re-checked.
- **Bounded read:** the catalog read is a pinned no-follow read bounded at 65536 bytes, with the sha checked against the inventory before decoding.
- **v1 catalogs:** a v1 catalog is refused as an unsupported shape and never executed.
- **Codecs:** the committed catalog names only the compiled `native` validator.

## Tests too weak to catch a plausible regression
- **`tests/t2_omp_install.rs`:** no test registers an `OmpActivation`. As a result these never execute: `observer_binding_with`, `verified_observer_binding`, `observe_observer_binding`, `manual_comparison_inner` (about 450 lines, including the link/copy pinning and every recheck), installed `diagnostics` and `answer_first`.
  - Deleting the `artifact_files` equality (`:581`), the receipt-fingerprint recheck, or any pin comparison still passes all 18 tests.
  - A test provider over a temporary state/pack would cover them, using `inspect_runtime_with` to build a matching receipt.
  - `rust_reissue_never_inherits_current_rollout_attestation` locks in P2-1.
- **`tests/t1_migration.rs`:**
  - No catalog has a cycle that passes through a readable target, e.g. `v1→legacy` plus `legacy→middle→v1`. `route` returns at the target before revisiting it, so only the `acyclic` pass (`migration_contract.rs:354-357`) refuses. Deleting that pass leaves the suite green.
  - The non-finite message (P3-2) is untested.
- **`tests/t2_bootstrap.rs`:**
  - Every fetch is `file://` with `=file`, so `--proto-redir =https`, `--max-redirs` and the real GitHub latest→tag→objects redirect chain are never exercised.
  - Only TERM is exercised; the HUP and INT traps are not.
  - The production signer line is checked only as a constant. That is unavoidable without the release key.
- **`tests/t2_cli.rs`:** has no successful `am artifact inspect`, which hides P2-3.
