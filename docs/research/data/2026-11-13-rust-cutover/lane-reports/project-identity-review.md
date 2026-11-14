# Project identity independent review

## Source and method

Reviewed source: `2457005f8447d32bd1f4958654001f323c29ea34`, branch `lane/reviews`. Reviewer: `ReviewProjectIdentity`, `anthropic/claude-opus-5-5:high`; report integrated by `omp-beast`.

Read in full: `src/project_identity.rs:1-1486`, `tests/t1_project_identity.rs:1-2082`, baseline `skills/agent-memory/scripts/agent_memory/project_identity.py:1-471`, `tests/contracts/test_project_identity.py:1-611`, author `projid-report.md`, ADR-0029, the cutover design and lane brief. Directly relevant filesystem, registry, repository-config, Git, codec, error and raw-recovery dependencies were inspected at call sites. No Python, build, runtime proof, test, lint or formatter was executed. Findings are static source evidence; inferred execution outcomes are not claimed as observed runs.

## Findings

### P1-1 — Nested bank roots make every project identity operation fail

**Rust evidence:** `src/project_identity.rs:547-552` joins `bank = root.join(settings.bank_root)` but checks it using `stat_optional(&root, OsStr::new(&settings.bank_root))`. `stat_optional:347-361` calls `Descriptor::stat_at`; `src/fs/descriptor.rs:299-300` calls `child_name`, which rejects any name containing `/` at `:68-76`. The resulting `InvalidInput("One ordinary path component is required")` becomes a hard `ContractError`, before an actual stat.

**Baseline/design:** `project_identity.py:150-153` checks the final bank/router components with `Path.is_symlink`, allowing nested paths. Both `repository_config.py:63-75` and `src/repository_config.rs:35-60` accept nested roots. `skills/agent-memory/sharing/boundary/GUIDE.md:22` explicitly documents `agent-memory.bankRoot 'docs/shared memory'` and identity's shared resolver. The baseline sharing fixture binds such a root (`tests/contracts/test_sharing_boundary.py:31-43,168,313`).

**Concrete failing input:** set `git config --local agent-memory.bankRoot 'docs/shared memory'` in a repository, with an otherwise valid bank. [INFERENCE from the direct call chain] `status`, `preview`, `apply`/replay evidence rechecks and raw recovery all fail with the component-name error, whether the bank exists or not. A uniquely discovered committed nested router reaches the same defect without explicit config. Downstream mediated writers inherit the refusal. This is a wrong result for a documented supported configuration, hence P1 under the requested rubric.

**Minimal fix:** inspect the final component under the bank's parent, using `bank.parent()` and `bank.file_name()` with ordinary typed refusals for missing components, then `stat_optional(parent, name)`. Retain descriptor/no-follow checks. A missing parent already maps to optional absence. Do not pass a relative multi-component path to a single-component syscall wrapper.

**Missed regression:** all accepted fixtures in `tests/t1_project_identity.rs` use single-component `memory-bank`, `project-notes` or `foreign-bank` roots; the slashed root at `:1979` is rejected earlier as unsafe label data. Add a real nested-root status/preview/apply/replay case plus raw recovery; source-string assertions would not exercise this failure.

Confidence: 0.95. Parent source inspection confirmed the named call chain; no runtime reproduction was run.

### P3-1 — Symlinked bank lock has the wrong refusal text

**Rust:** `src/project_identity.rs:565-573` returns `Bank busy or interrupted; inspect/recover before resolving identity` immediately for a symlinked `.agent-memory.lock`, including when `owned_lock` is absent, bypassing the canonical-writer probe.

**Baseline:** `project_identity.py:159-163` sends an unowned bank lock to `local_registry.check_canonical_writer` first. `local_registry.py:44-50` refuses a symlink with `Unknown or unsafe canonical writer marker; preserve for inspection`. The corresponding Rust probe (`src/local_registry.rs:322-335`) preserves this text, but this call site never reaches it.

**Input:** a symlink at `memory-bank/.agent-memory.lock`, followed by status. Both implementations refuse without mutation, but meaningful refusal strings differ, contrary to the lane brief.

**Minimal fix:** defer the unowned `.agent-memory.lock` symlink case to `check_canonical_writer`; retain the existing early symlink refusal for transaction markers and owned-lock cases. No weaker path read is needed.

**Missed regression:** the current unknown-bank/busy-state test symlinks the registry, not a bank marker. Add the actual symlinked-lock case using the real writer-operation adapter.

Confidence: 0.85. Severity P3 because the safe refusal/exit outcome is unchanged; this is message parity only.

### P3-2 — Busy-state obligation asserts missing wiring instead of its consumer outcome

**Rust test:** `tests/t1_project_identity.rs:1208-1215` writes `Writer` into `.agent-memory.lock`, then asserts `provider is not registered`. It does not exercise the intended ordinary busy-marker refusal. `src/project_identity.rs:576-585` is the missing `BankWrites` registration path; the operations are already available in `src/bank_writes.rs:15`.

**Baseline:** `tests/contracts/test_project_identity.py:592-595` expects the real status refusal; `project_identity.py:160-167` always supplies `bank_writes.OPERATIONS`. The invalid marker is refused as unsafe or malformed, depending on its mode, not because a provider is absent.

**Concrete regression it misses:** production wiring could remain unregistered or canonical-writer Kind mapping could regress while this obligation still passes by asserting the implementation seam.

**Minimal fix:** register the real `WriterOperations` adapter as the dedicated typed-writer test does at `tests/t1_project_identity.rs:2034-2039`, then assert the actual invalid-marker refusal. Keep missing-registration coverage separate from migrated consumer obligations. Include P3-1's symlink variant.

Confidence: 0.8.

## Coverage and integration limits

- No nested-root or symlinked-bank-marker regression exists; the ordinary busy obligation checks the wrong path, as above.
- `command_result` has no direct byte/exit/stderr coverage for indent-2 UTF-8 success, ASCII default-spaced failure, capacity details or empty stderr. The library tests do not exercise proposal-file decoding; that remains CLI-owned.
- The ledger calls `test_copied_pack_spaces_non_git_project_and_unrelated_registry_preserved` ported, whereas the author report correctly calls relocation/executable coverage partial. This suite's ported entries have `rust_test: null`.
- Raw recovery uses local test adapters because real `layout_recovery_observation` is not ported. The 13 `test_repository_observation_batch.py` mappings were inspected via Rust tests/report, not compared against every baseline body.
- Threaded flock races exercise independent descriptors, but not distinct owner PIDs. `OUTER_LAUNCH` serialization is test-only, not a runtime remedy.
- Required registration seams are allowed by the design, but `BankWrites`, `BankObservation` and `LayoutObservation` must be wired before consumer shipment. CLI composition is not independently accepted here.
- Ordinary/raw router reads are bounded at 1 MiB and registries at 64 MiB; canonical v1 router validation retains 16,384 bytes. The 1 MiB owned-marker comparison is smaller than the recovery witness's 512 MiB bound. Whether every capacity-valid transaction marker fits 1 MiB was not proven; no unsupported over-limit finding is asserted.
- The remote loop's `e.contract().is_some()` would include a cleanup error with a contract primary, but `identity_git::git` does not currently produce that variant there. This is an untested future edge, not a reachable defect finding.
- Approved deltas: strict UTF-8/typed malformed-input refusals, fixed bounded reads, and registered traits for unavailable modules. Multiple-corruption error precedence and Python bool/number equality differences were not promoted to findings.

## Verdict

**Changes required: P1-1 blocks acceptance.** Two additional P3 parity/test defects should be corrected. Review confidence 0.85. No code was changed by the reviewer. Runtime and cross-platform proof were not run; macOS/Linux conclusions are limited to source-level syscall contracts. The P1 component-name defect is platform-independent.
