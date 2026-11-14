# Gate C report — Python removal (lane `gatec`)

Worktree `/opt/agent-memory-worktrees/rust-v015-lane-gatec`, base `5aacd386` (detached). No Git writes: deletions are unstaged worktree removals, edits are uncommitted. Main stages with `git add -A`.

## Deleted paths (572 tracked paths)

| Directory | Count | What |
|---|---:|---|
| `skills/agent-memory/scripts/` | 140 | runtime package `agent_memory/*.py` + wrapper scripts |
| `skills/agent-memory/hooks/` | 8 | 7 `hooks/*.py` + `hooks/python3.sh` |
| `scripts/` (top level) | 42 | 41 `scripts/*.py` (incl. `test_gate.py`) + `scripts/test.sh` |
| `scripts/evaluations/` | 15 | evaluation `.py` entry points (#206 moved to v0.0.16; Git history keeps them) |
| `scripts/distribution/` | 3 | `release.py`, `README.md`, `install.sh.in` (Python-era publisher) |
| `tests/` (top level) | 24 | 23 `tests/*.py` fixtures (incl. `v014_runtime_fixture.py`) + `tests/contracts.json` |
| `tests/contracts/` | 250 | Python contract suites |
| `tests/fixtures/legacy-cli/` | 3 | `pre-am-3af48ea.tar.gz`, `provenance.json`, `README.md` |
| `tests/fixtures/v0.0.14-runtime/` | 2 | `agent-memory.tar.gz`, `provenance.json` |
| `tests/fixtures/v0.0.2-runtime/` | 2 | `portable-pack.tar.gz`, `provenance.json` |
| `docs/research/` | 80 | research `.py` (data, receipts and prose kept) |
| `data/2026-11-05-ssg-runtime/` | 3 | `licenses/*.py` |

Before deleting, `rg` over `src/` and `tests/**/*.rs` found no Rust reader of the three fixture archives or of `tests/contracts.json` (only historical `docs/research` source bindings name them). All three archives were deleted; none had to be kept. `tests/fixtures/v014-state` is kept unchanged. `.gitignore` lost `__pycache__/` and `*.py[cod]` and now holds only `/target/`. Empty directories (`scripts/`, `tests/contracts/`, `skills/agent-memory/scripts/`) were removed, and so were leftover untracked `__pycache__` directories.

## Retargeted identifiers

**Command surface (`agent-memory/command-surface-v1`).** The schema id stays the same: no signed surface has been published yet.
- `cli::Helper.script` (`"developer_identity.py"`, …) became `Helper.command`, the native command `am <name>`. The name is taken from the helper's first table route that has no fixed prefix, falling back to its first route. Resulting names: `am admission`, `am artifact`, `am context`, `am authority-policy`, `am bank`, `am bank-layout`, `am inspect`, `am transaction`, `am writes`, `am checkpoint`, `am claude-boundary`, `am codex-boundary`, `am compare`, `am compact`, `am developer`, `am discover`, `am govern`, `am inactive-records`, `am init`, `am recover-layout`, `am lifecycle-model`, `am publish`, `am refresh`, `am migrate`, `am omp-boundary`, `am validate-pack`, `am private`, `am project`, `am projection`, `am record-contract`, `am search`, `am identity`, `am scope-policy`, `am security-policy`, `am semantic`, `am sharing`, `am exchange`, `am tombstones`.
- `surface::ROOT_SCRIPT = "installer.py"` became `surface::ROOT_COMMAND = "am"` (the root module stays `installation`).
- The declaration value now carries `helpers[].command` instead of `helpers[].script`, and `root.command` instead of `root.script`. `routes[].helper` is now the helper's command.
- `Declared` is keyed by command. It no longer builds `scripts/<x>.py` keys or `scripts/agent_memory/<m>.py` modules: `Dispatch.module` is the plain Rust module name, which is also the `module` in pack-validation readback rows. Decoding validates every helper command as `am <lowercase-name>` and requires the root command to be exactly `am`.
- `Declared::scripts()` became `Declared::helper_commands()`.
- `pack_validation::CommandSurface::dispatch(&self, script, files: &Inventory)` became `dispatch(&self, command)`, because the inventory parameter was a leftover of the old AST parsing. `check_dispatch` lost its `files` parameter and now resolves delegated routes by command. Both `NoExecutableSurface` fixtures were updated (`src/bin/am_dev/release.rs`, `tests/t2_delivery.rs`).
- Pack guides needed no change: they already document `am …` only. The pack-validation refusal of `python3`, `*.py` and `python3.sh` invocations is unchanged, including the quoted `$SKILL_DIR/...` refusal vectors.

**`with_validated` signature.** `pack_validation::with_validated(pack, body)` now gives the body `&dyn CommandSurface` (the surface it already built), so artifact inspection does not build a second one. The callers updated are `artifact::{inspect, inspect_runtime}`, `pack_validation::validate` and `tests/t1_a2.rs`.

**Optional adapter.**
- `adapters/github-publication.json` changed from `optional-adapter-v1` with `"entrypoint": "scripts/publish_memory.py"` to `optional-adapter-v2` with `"entrypoint": "am publish"`.
- `artifact::adapter_entry` now takes the command surface. It requires schema `agent-memory/optional-adapter-v2` and an entrypoint of the form `am <name>` whose `<name>` the signed surface declares (`CommandSurface::command`).
- With no registered surface it refuses: "Signed command surface provider is not registered; adapter entrypoint validation refuses".
- `inspect_validation`, `inspect_with` and `inspect_runtime_with` pass the surface through.

**Runtime compatibility.**
- `schemas/runtime-compatibility.json` dropped `python_minimum` and `required_python_packages`.
- `artifact.rs` dropped `fn python_minimum` and both checks. Its refusal text now reads "…and a client-neutral core" instead of "standard-library core".
- The schema id stays `agent-memory/runtime-compatibility-v1`, without a bump. Removing two required fields only loosens the Rust reader. Python-era receipts that still carry the keys remain readable as adoption inputs. The Python runtime never reads a v0.0.15 pack, because it refuses the `release-v2` manifest first.
- Test constants were updated in `t1_a2`, `t2_delivery`, `t2_diagnostics`, `t2_installation`, `t2_lifecycle`, `t2_migration`, `t2_omp_transport`, `t2_recall_catalog` and `release.rs`. Left as they are: `t2_lifecycle.rs:1166`, a Python-era `release-v1` legacy-manifest fixture, and the captured `v014-state` receipts and journals.

**Incidents.**
- `tests/incidents.json` lost `suite`/`class`/`tests[name]`. Each entry now has `tests: [{"target", "test"}]`, mapped as follows:
  - `t2_governance::journey_legacy_flat_consumed_capture_stays_pending`
  - `t2_sharing_recovery::test_reachable_side_parent_discarded_by_main_is_not_a_publication_witness`
  - `t2_sharing_recovery::test_actual_merge_tree_retaining_delta_is_a_witness_even_after_later_correction`
- `am-dev run-incident` (`incidents.rs::native_test`) reads `tests/<target>.rs`, using a bounded read and validated identifiers. It requires the named `fn` to be directly bound to an `// obligation:` annotation, exactly once.

**Hooks and adapters.** `hooks/omp.ts` and the OpenCode adapters had no Python or `boundary_ledger.py` reference left, so nothing changed there. Two Rust doc comments that named the deleted launcher were reworded: `src/hooks.rs` and `src/cli/helpers/hooks.rs`.

**Test-world fixture mapping.** The translation is deleted (see "Follow-up: test-world translation removed" below). Every fixture call now names its native `am …` command.

**Docs.**
- `scripts/test.sh` was deleted. The cargo gate is the CI workflow's `cargo test --locked --no-fail-fast --features dev-tools`, and no current doc referenced `test.sh`.
- `docs/testing/public-release.md` gained "Publishing the signed release", which documents `am-dev release publish|build` (`--binary TRIPLE=PATH` ×3, `--private-key`, `--integration-branch`, `--notes-file`/`--output`, `--root`) in place of `scripts/distribution/README.md`.
- `docs/testing/canaries.md` now points at the `c_test_release_canaries` and `c_test_identity_canaries` targets instead of `tests/contracts.json`.
- `docs/testing/incidents-and-guides.md` describes the native incident mapping and `am-dev run-incident`.

## Schema changes
- `agent-memory/optional-adapter-v1` → `agent-memory/optional-adapter-v2`: `entrypoint` is an `am <command>` declared by the signed command surface instead of a `scripts/` file in the pack.
- `agent-memory/runtime-compatibility-v1`: `python_minimum` and `required_python_packages` are removed and no longer checked; the version is not bumped (reason above).
- `agent-memory/command-surface-v1`: `helpers[].script` → `helpers[].command`; `root.script` → `root.command` (`"am"`); route `helper` values are commands. The id is not bumped because no signed surface has been published.
- `agent-memory/incident-catalog-v1`: `suite`/`class`/string `tests` are replaced by `tests: [{target, test}]`. The id is not bumped: the catalog is developer-only and has no other reader.

## Tests changed or added
- Added `tests/payload_audit.rs` (3 tests); see below.
- Un-ignored the two publisher success tests in `src/bin/am_dev/gaps_publisher.rs` and their wrappers in `tests/c_test_gaps_publisher.rs` (`needs Gate C Python removal (native adapter entrypoint)`).
- Removed the lane-local Python-free pack filters, which are now no-ops:
  - the `__pycache__`/`.pyc`/`.pyo` skip in `tests/support/cutover.rs::copy_tree`;
  - the `__pycache__` filter in `src/bin/am_dev/capture_examples.rs`.
  The release builder's refusal of `.py`, `.pyc`, `.pyo` and `__pycache__` in tracked pack entries stays, because it is a refusal rather than a filter.
- Retargeted tests:
  - `tests/t2_cli.rs`: the declaration-equals-table test now asserts the primary-command rule, plain module names, root `am`, and the absence of `.py`, `scripts/` and `python` in the canonical declaration. The native-guide surface fixtures use commands. `native_surface_resolves_only_commands_in_the_table` was renamed from `…_without_pack_scripts`.
  - `tests/c_test_portable_artifact.rs`: asserts that the Python keys are absent and that the entrypoint is `am publish`.
  - `tests/t1_a2.rs`: `with_validated` closure arity.
  - `tests/support/world.rs`.

## Payload audit result
`tests/payload_audit.rs` passes on macOS and Linux (see Runs). It has three tests:
1. `repository_tree_holds_no_python_payload`
   - Tree source: `git ls-files -z --cached --others --exclude-standard`, run with system/global Git config disabled and filtered to paths present on disk, so it covers what `git add -A` would commit. In a copy without `.git` (the Linux run), it walks every file except `target/`.
   - Path rule: refuses `.py`, `.pyc`, `.pyo`, `.pyd` and `.pyw` files, any `__pycache__` component, and `python`/`python3`/`python*.sh` launcher names.
   - Content rule: refuses a `#!…python` shebang in any file, with no allowlist. It also refuses `python3` or `#!/usr/bin/env python` text outside the allowlist.
2. `portable_pack_and_built_release_archive_hold_no_python_payload`
   - Runs the same path and content rules, with no allowlist, over `release::portable_files(skills/agent-memory)`.
   - Then runs them over the archive built by `release::encode_archive` (the builder's encoder) after gunzip and `release::tar_entries`, and checks that the archive members equal the inventory.
3. `audit_detects_every_python_payload_form`: negative controls.

As an extra negative control, I created an untracked `tests/probe.py`; the tree test failed on it as expected, and I removed the file.

The allowlist covers text only, never paths or shebangs:
- historical `docs/research/`, `data/`, `memory-bank/`, `docs/adr/` (ADR history);
- `docs/design/v0.0.1-*`, `v0.0.3-*`, `v0.0.14-*`;
- `docs/design/v0.0.15-omp-integration.md` (pre-cutover design history) and `docs/design/v0.0.15-rust-cutover.md` (names the Python-era state it recognises);
- released `docs/releases/*` notes;
- `docs/releases/v0.0.15.md` from the `## Superseded Python candidate record (historical)` heading onward only, as a historical superseded record section of the release notes. Both stale `python3 -I` lines (248 and 520) sit inside that section; text above the heading is still audited. This follows Main's decision.
- `docs/testing/observed-guide-examples.json` (retained capture of the superseded runtime, documented as historical);
- the pack-validation refusal vectors: `src/pack_validation.rs`, `tests/t2_cli.rs`, `tests/c_test_read_only_audits.rs`.

The allowlist also has entries beyond the assigned list:
- Python-era adoption recognisers and their captured inputs: `src/installation/adoption.rs`, `src/client_hooks.rs`, `tests/fixtures/v014-state/`, and the adoption vectors in `tests/t2_hooks.rs`, `tests/t2_installation.rs`, `tests/t2_leaves.rs` and `tests/t2_diagnostics.rs`. The design requires recognising the exact prior `/bin/sh …/python3.sh …/*.py` hook commands and wrapper bytes, and the kept `v014-state` captures contain them. They are inert data and refusal/rewrite inputs, not launchers.
- The audit test itself.

## Left, with reason
- `docs/research/data/2026-11-05-ssg-runtime/workspace-critic/fixture/requirements.txt` (`Flask==3.1.2`) is kept. It is a historical research fixture for static fingerprinting, never installed, and it is not under the deleted `.py` set.
- Python names in Rust doc comments (`Baseline \`x.py\``) and in `// obligation: tests/contracts/*.py::…` annotations are kept. They are provenance and the frozen ledger id format, which the brief requires to stay unchanged.
- `tests/obligations/python-baseline-v1.json` is unchanged: every id is kept and no status was edited. Status updates belong to Main.
- `src/bin/am_dev/release.rs` keeps its Python-file refusal for tracked pack entries, as defence in depth.

## Runs
- `cargo fmt`: applied.
- `cargo check --offline --all-targets --features dev-tools`: 0 warnings, 0 errors (macOS, aarch64-apple-darwin, Darwin 27.3.0, rustc 1.99.1).
- `cargo test --offline --no-fail-fast --features dev-tools` on macOS arm64: exit 0 across 145 test binaries, with 1471 passed, 0 failed and 789 ignored. The ignored tests are pre-existing `needs …` ignores outside this lane.
  - `payload_audit`: 3/3 passed.
  - Both publisher success tests (lib `release_builder::gaps_tests::*` and their `c_test_gaps_publisher` wrappers) passed. Run alone, the lib publisher tests passed earlier as well (3 passed, 74.8 s).
- `linux-run.sh /opt/agent-memory-worktrees/rust-v015-lane-gatec gatec --features dev-tools`: exit 0. Output: `LINUX OK (aarch64-unknown-linux-musl run, x86_64 musl built)`. The VM runs Lima kernel 6.8.0-149-generic. That run includes `payload_audit` 3 passed, `c_test_gaps_publisher` 3 passed and the lib suite 22 passed. It audits the tree by the no-`.git` walk path.
- Acceptance check: `git ls-files | grep -E '\.py$|\.pyc$|python3\.sh$'` is empty once Main stages the 572 deletions (`git add -A`). Before staging, the index still lists them, but none of them is on disk.

## Translation removal

The test-world shim that translated baseline script names (`"bank_writes.py"` → `am writes`) is deleted. Every fixture call now passes its native command path literally.

**What changed**
- Codemod (ast-grep through `xd://ast_edit`): 169 string-literal replacements in 20 test files, applied by the same mapping the shim used, e.g. `"bank_writes.py"` → `"am writes"`, `"scope_binding.py"` → `"am identity"`, `"reconcile_memory.py"` → `"am semantic"`, `"migrate_bank.py"` → `"am migrate"`, `"recover_layout.py"` → `"am bank recover-layout"`. Arguments are untouched.
- The shim's three special cases were converted by hand, moving the consumed prefix operand into the command (7 sites):
  - `"bank_state.py", ["inspect", …]` → `"am inspect", [ …]` (2);
  - `"admit_learning.py", ["capture", …]` → `"am capture", [ …]` (3);
  - `"installer.py", ["install", …]` → `"am install", [ …]` (2).
- A second codemod renamed the callers: `World::helper` → `World::am_json` and `helper_arguments` → `native_arguments` (10 replacements in 6 files). Wrapper parameters named `helper`/`script` became `command` in `tests/support/{cutover,identity_fixtures}.rs`, `tests/t2_publication.rs` and `tests/c_test_authority_boundary.rs`.
- `tests/support/world.rs`: the 136-line `helper_arguments` mapping table and its registration checks are removed. In their place, `native_arguments(command, args)` only splits a literal `am …` command path into argv and refuses anything else with "Fixture command must be a native am invocation".
- `tests/t_support_world.rs` retargets the two fixture tests. One now asserts that a script name is refused, that `am bank recover-layout` and bare `am` split correctly, and that a non-`am` prefix is refused. The other, the identity fixture test, drops its "not registered" branch.
- Calls with no native equivalent: none. Every mapped name had one.

**Remaining `rg 'scripts/[a-z_]+\.py' tests` hits: 349, none of them a call site**
- 336 in `tests/fixtures/v014-state/*/state/agent-memory/install/{receipt,journal}.json`: captured Python-era `artifact_files` inventory keys, kept byte-exact as adoption inputs.
- 3 in `tests/obligations/python-baseline-v1.json`: the frozen ledger.
- 0 in `// obligation:` annotations, which use `tests/contracts/*.py` and so do not match this pattern.
- 10 others, none of them a call site:
  - pack-validation refusal vectors: `c_test_read_only_audits.rs:174-176`, `t2_cli.rs:1303,1307`;
  - negative controls: `t2_cli.rs:713` (an undeclared helper key is refused), `t1_a2.rs:441` (candidate code written into a pack must never execute), `payload_audit.rs:296` (the audit must flag a `.py` path);
  - a provenance comment (`t2_migration.rs:665`);
  - the captured argparse vector note (`tests/vectors/cli-argparse-v1.json:5`).

**Runs after the conversion** (macOS arm64, aarch64-apple-darwin, Darwin 27.3.0, rustc 1.99.1)
- `cargo fmt --check`: clean. `cargo check --offline --all-targets --features dev-tools`: 0 warnings.
- `cargo test --offline --no-fail-fast --features dev-tools`: exit 0 across 145 binaries, with 1472 passed, 0 failed and 789 ignored (all pre-existing `needs …`). No source or test file changed after this run.
- `linux-run.sh /opt/agent-memory-worktrees/rust-v015-lane-gatec gatec --features dev-tools`: exit 0, `LINUX OK (aarch64-unknown-linux-musl run, x86_64 musl built)`, 120 test binaries. Among them: `t_support_world` 19 passed/1 ignored, `payload_audit` 3 passed, `c_test_gaps_publisher` 3 passed, and the lib suite 22 passed.
