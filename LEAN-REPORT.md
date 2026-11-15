# v0.0.15 lean `am` — rebuild report

Branch `lean/v0.0.15`, built against [docs/design/v0.0.15-lean.md](docs/design/v0.0.15-lean.md).
Nothing is committed.

## Line count (product)

| | Before | After |
| --- | ---: | ---: |
| Rust `src/` (am + am-dev) | 133,453 | **5,183** |
| `tests/` | 189,253 (209 files) | 0 |
| Skill pack `skills/agent-memory/` | 9,285 (71 files) | 648 (14 files) |
| `install.sh` | 369 | 356 |

Rust by file: `install/mod.rs` 1,315 · `install/release.rs` 753 · `share.rs` 507 ·
`bank.rs` 455 · `json.rs` 451 · `memory.rs` 424 · `doctor.rs` 290 · `util.rs` 271 ·
`main.rs` 242 · `repo.rs` 159 · `hook.rs` 57 · `lib.rs` 10 · `bin/am_dev/main.rs` 249.
Dependencies: `sha2`, `flate2` (dropped `libc`, `sha1`, `regex`, `unicode-normalization`).

## Build checks

- `cargo build --release`: 0 warnings (also with `--features dev-tools` for `am-dev`).
- `cargo fmt --check`: clean.
- `aarch64-unknown-linux-musl` and `x86_64-unknown-linux-musl` with
  `CARGO_TARGET_<TRIPLE>_LINKER=rust-lld`: 0 warnings; `file` reports statically
  linked / static-pie ELF.
- All builds used `CARGO_TARGET_DIR=/tmp/lean`.

## Commands

`am status`, `am doctor`, `am init`, `am recall [QUERY]`, `am remember`,
`am share`, `am refresh`, `am install`, `am update [--check]`,
`am uninstall [--dry-run]`, `am hook session-start --client claude|codex|omp`,
`am --version`, `am help`. Release tooling: `am-dev release build|publish`.

## User smoke test (macOS arm64, temp HOME)

`scripts/qa.sh <worktree>` (new, kept in the repo) drives the real binary. Final
run: **0 failures, 24 steps (21 `am` commands, 3 plain git commands) + 23 checks**.

| Step | Time |
| --- | ---: |
| `am install --development-source <worktree> --presets agents,codex,claude,opencode,omp --omp-agent-dir …` | 173 ms |
| `am init` | 6 ms |
| `am remember` (flags) | 15 ms |
| `am remember` (duplicate → skipped) | 6 ms |
| `am remember` (stdin JSON) | 19 ms |
| `am recall` | 7 ms |
| `am recall musl linker` | 6 ms |
| `am share` (clone A → bare remote) | 170 ms |
| `am refresh` (clone B) | 121 ms |
| `git pull` (clone B, after refresh; fast-forward) | 41 ms |
| `am remember` (clone B) | 15 ms |
| `am share` (clone B) | 138 ms |
| `am refresh` (clone A) | 92 ms |
| `am share` (nothing new) | 82 ms |
| `git pull` (clone A, after share and refresh; fast-forward) | 43 ms |
| `am refresh` (clone C, own local commit) | 127 ms |
| `git commit -m "Shared memory"` (clone C) | 13 ms |
| `git pull --no-edit` (clone C, true merge) | 48 ms |
| `am hook session-start --client claude` | 11 ms |
| `am hook session-start --client omp` | 6 ms |
| `am hook session-start` outside a repo | 2 ms |
| `am status` | 6 ms |
| `am doctor` | 6 ms |
| `am uninstall` | 7 ms |

Checks that passed: `am` on PATH is the installed link; Claude and Codex
`SessionStart` hooks registered; an existing Claude setting kept; omp extension
written; skill linked; duplicate skipped; recall finds the note; after share
only `memory-bank/` is staged in A and its HEAD did not move; remote `main` has
the bank; B received A's note and A received B's; `MEMORY.md` links all three
notes; after a plain `git pull` B and A are clean and at `origin/main`; C (with
its own commit) merged cleanly and kept its work and the notes; hook emits
`additionalContext`; uninstall removed the commands and omp extension, restored
Claude settings, removed the Codex `hooks.json` that was absent before, and left
banks untouched.

Every command is under the 200 ms target (share/refresh include local `git
fetch`/`push` to the bare remote); the session-start hook is 2–11 ms (target 100 ms).
`install` spends most of its time in `fsync` (F_FULLFSYNC on macOS) for the
eight atomically replaced files.

### Additional runs (ad hoc, temp HOME, fixtures removed afterwards)

- **v0.0.14 takeover** using a copy of this machine's real Python-era install
  (receipt-v1, source tree, wrappers, links, activation blocks, Claude/Codex hook
  files with paths moved into the temp HOME): the lean directory fingerprint
  matches the v0.0.14 receipt identities exactly (checked against the live
  receipt, read-only). `am install --development-source` took over in 59 ms
  (`--dry-run` 176 ms): links and wrappers replaced, activation blocks replaced
  in place, Python `UserPromptSubmit`/`Stop` entries removed and `SessionStart`
  added (other hooks untouched), OpenCode plugin link and Python source tree
  removed. `am uninstall` (29 ms) then left `settings.json`, `hooks.json` and
  `CLAUDE.md` equal to their pre-v0.0.14 content (the diff is exactly the old
  agent-memory entries and block).
- **Signed release path** in a scratch copy with a throwaway signing key and
  `file://` asset URLs (test-only patch, not in the product): `am-dev release
  build` for all three targets; the patched `install.sh` downloaded, verified
  with `ssh-keygen` and ran `am install --bootstrap-binary` (338 ms total);
  `am doctor` verified the runtime against its signed manifest; `am update
  --check` 27–30 ms; `am update` to a 0.0.16 build 60 ms (binary signature,
  manifest, archive, pack identity verified); `am uninstall` 6 ms.
- **Conflicts and protected branches** on a bare remote: overlapping edits to a
  context file produce conflict markers on refresh, `am share` refuses until
  they are resolved, then shares; a pre-receive hook rejecting `main` with
  `GH006` made `am share` push `agent-memory/<project-id>/<commit>` and try
  `gh pr create` (gh refused for lack of a GitHub remote, reported plainly).
- **Existing bank**: `am recall` and `am doctor` on this repository's real
  `memory-bank/` (read-only) — parsed and healthy, 7 active lessons; recall 8–13 ms
  warm, hook 8 ms.

## What was deleted

- **All of `src/` except the rewritten modules**: the 130k-line codebase including
  omp boundary/stream/transport/projection, Claude/Codex boundaries and turn
  sources, turn hooks, hook-failure and boundary ledgers, seals/nonces/clock
  windows/wake diagnostics; layout recovery (all `layout_*`), recovery
  frontier/quiescence/transactions, sharing holds/frontier/recovery/limits/
  semantic sharing/page export; authority, governance, admission, assembly,
  comparison, consolidation, checkpoint, handoff, tombstones, purging,
  migration, scope binding, project/developer identity registries, private
  state, local registry, cache recovery, validation memo, unicode tables,
  process/fs descriptor layers, runtime faults and the codec.
- **`tests/`** (209 files) and every `#[cfg(test)]` module.
- **Obligation ledger** (`tests/obligations/python-baseline-v1.json`) and
  `am-dev` test-only tools: `obligations`, `incidents`, `guide_evidence`,
  `capture_examples`, `gaps_publisher`, `omp_smoke`. `am-dev` keeps only
  `release build|publish` (249 lines).
- **Install machinery cut**: omp seal/peer-reader activation, layout quiescence
  and frontier holds, authority checks, journal files, owner records, managed
  Git channels, the Python-era exact-wrapper reconstruction (replaced by the
  receipt's own identities), the command-surface comparison.
- **Skill pack**: every guide (44 `GUIDE.md` files) — the lean workflow lives in
  the rewritten `SKILL.md`; `hooks/boundary-prompt.txt`, the OpenCode turn
  plugins, `adapters/`, ten cut schemas, the old activation/router/record
  templates. Added `templates/memory-bank/*` (init), `templates/activation-markdown.txt`
  and `templates/remember.example.json`; `hooks/omp.ts` is now 36 lines.
- **Dead dependencies**: `libc`, `sha1`, `regex`, `unicode-normalization`.
- **Docs/CI**: `docs/testing/` (described the deleted suites); the contract-test
  workflow replaced by `.github/workflows/build.yml` (fmt, warning-free build per
  target, `scripts/qa.sh`). `README.md` and `docs/releases/v0.0.15.md` rewritten;
  `install.sh` drops the quiescence, peer-upgrade, `--remote/--ref`, `--json`
  and `--verbose` options.

## Not done, and why

- **Linux QA runs.** No Linux host or VM was available (Podman and Lima have no
  machine). The Linux binaries are cross-built and static, but `scripts/qa.sh`
  has only run on macOS arm64. The CI workflow runs it on all three targets.
- **Signed release against GitHub.** Needs the private release key and a
  published release; exercised end to end only with a throwaway key and
  `file://` URLs in a scratch copy (above).
- **Real omp sessions.** The omp extension is untested inside omp; it relies on
  `session_start` (spawns `am hook` once) and the first `before_agent_start`
  returning a hidden message, the API the old `omp.ts` used. omp cannot inject
  context from `session_start` itself, so each later prompt does one map lookup
  and returns nothing — no process, no I/O.
- **Hook git use.** The spec says the session-start hook uses no git beyond
  finding the root. The root is found without git (walking up to `.git`); the
  hook then makes the one per-command `git config` call so a configured
  `agent-memory.bankroot` is honoured (2–11 ms total).
- **Crash recovery of install.** Install/update/uninstall roll back in process
  (an undo log restores every replaced file, link and directory) and write the
  receipt last; there is no on-disk journal, so a `kill -9` mid-install can leave
  new targets without a receipt. A re-run adopts targets that already equal the
  plan (links, the omp extension, activation blocks, hook entries) but refuses a
  half-moved runtime directory until it is removed.
- **Refresh and `git pull` (fixed per Main's decision).** `am refresh` and a
  successful `am share` now stage the shared commit's `memory-bank/` in the
  index (`git reset -q <shared-commit> -- memory-bank`; index only, so local
  unshared edits and new notes stay in the working tree). A plain `git pull`
  that fast-forwards then succeeds (QA clones A and B). Git refuses any
  non-fast-forward merge while the index differs from HEAD, so on a branch with
  its own commits the user first commits the staged memory
  (`git commit -m "Shared memory"`); the identical files then merge cleanly
  (QA clone C). `refresh`/`share` print that hint. Because the memory is
  staged, a plain `git commit` also includes it.
- **OpenCode.** It keeps its skill link and activation block; its turn plugin is
  removed (the spec's session-start hooks are Claude, Codex and omp).
- **Python-era backups.** v0.0.14 `pre-activation-snapshot` backups recorded in
  old receipts are left in place, neither restored nor deleted.
- **Rust receipt-v3** (pre-lean development installs) is read by the same
  takeover code but was not exercised.
- **Superseded docs** (`docs/contracts/`, ADR-0013 and later, `docs/research/`,
  older release notes) are kept as history; the lean spec replaces them.
