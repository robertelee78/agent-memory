# v0.0.15 lean `am` — rebuild report

Branch `lean/v0.0.15`, built against [docs/design/v0.0.15-lean.md](docs/design/v0.0.15-lean.md).
Committed through review batch 4 (c557117b); batch 5 below is not.

## Line count (product)

| | Before | After |
| --- | ---: | ---: |
| Rust `src/` (am + am-dev) | 133,453 | **6,000** |
| `tests/` | 189,253 (209 files) | 0 |
| Skill pack `skills/agent-memory/` | 9,285 (71 files) | 648 (14 files) |
| `install.sh` | 369 | 356 |

Rust by file: `install/mod.rs` 1,634 · `share.rs` 804 · `install/release.rs` 782 ·
`bank.rs` 456 · `json.rs` 451 · `memory.rs` 404 · `doctor.rs` 323 · `main.rs` 321 ·
`util.rs` 298 · `repo.rs` 194 · `hook.rs` 74 · `lib.rs` 10 · `bin/am_dev/main.rs` 249.
Dependencies: `sha2`, `flate2`, `libc` (only `geteuid` for the temp-dir check;
already in the lockfile through `sha2`). Dropped `sha1`, `regex`, `unicode-normalization`.

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
run after review batch 5: **0 failures, 40 steps (36 `am`, 4 plain git; 2 are
expected refusals) + 51 checks**. Timings below are from the batch-2 run; the
batch-3 run matched them (share 124–156 ms, refresh 73–135 ms, install 74 ms,
`install --dry-run` 128 ms, uninstall 28 ms, takeover 28 ms).

| Step | Time |
| --- | ---: |
| `am install --development-source <worktree> --presets agents,codex,claude,opencode,omp …` (linked Claude settings, interrupted-install leftovers present) | 152 ms |
| `am remember --help` | 3 ms |
| `am init` / `am init` on the existing bank (no change) | 7 / 6 ms |
| `am remember` (flags / duplicate / stdin JSON / internal heading) | 17 / 6 / 19 / 14 ms |
| `am recall` / `am recall musl linker` | 7 / 7 ms |
| `am share` (clone A → bare remote) | 151 ms |
| `am refresh` (clone B) | 127 ms |
| `git pull` (clone B, fast-forward) | 39 ms |
| `am remember` / `am share` (clone B) | 14 / 124 ms |
| `am refresh` (clone A) | 81 ms |
| `am share` (nothing new) | 53 ms |
| `git pull` (clone A, fast-forward) | 42 ms |
| `am refresh` / `git commit` / `git pull --no-edit` (clone C, own commit) | 156 / – / 67 ms |
| `am share` of an edited MEMORY.md row (A) → `am refresh` (B, own local row) | 140 / 93 ms |
| `am refresh` with staged-only memory (B) — **refused** | 67 ms |
| `am refresh` after `git add` (B) | 83 ms |
| `am refresh` with `memory-bank/topics -> ../src` (D) — **refused** | 77 ms |
| `am init` / `am share` with bank named `:` | 7 / 170 ms |
| `am doctor` on a bank without four context files | 7 ms |
| `am hook session-start` claude / omp / outside a repo | 15 / 7 / 2 ms |
| `am status` / `am doctor` | 6 / 7 ms |
| `am uninstall` | 7 ms |
| v0.0.14 takeover (offline fixture, `~/.claude` linked) / uninstall | 31 / 16 ms |

Checks that passed include: installed link on PATH; Claude/Codex SessionStart
hooks; the linked `settings.json` stays a link and keeps the user's setting;
leftover `.<name>.am-<uuid>` temporaries removed; `am install -h` usage; init on
an existing bank says so and changes nothing; harness recorded as `unknown`
without `--harness`; text before an internal heading kept; share stages only
`memory-bank/` and leaves HEAD; notes travel both ways; plain `git pull` clean;
B took A's row edit and kept its own row; staged-only memory survives the
refusal; nothing written into `src/` through the link; the `:` bank shared
without the modified/new code files beside it; the hook asks before saving;
uninstall restores Claude settings through the link, deletes the CLAUDE.md that
held only the block and leaves no temporaries; takeover replaces Python hooks
and wrappers, removes the old source, keeps `~/.claude` a link, and uninstall
leaves only the user's CLAUDE.md text and setting.

Every command is under the 200 ms target (share/refresh include local `git
fetch`/`push`); the session-start hook is 2–15 ms (target 100 ms).

Git calls (counted with a PATH shim): `am refresh` 4 when up to date, 10 with
changes (config, symbolic-ref, fetch, cat-file ×3, status, diff, update-ref,
reset); `am share` 14 with a new note (config, symbolic-ref, fetch, cat-file
×2, status, read-tree, add, diff, write-tree, commit-tree, push, update-ref,
reset), 9 with nothing new. Before: up to 19 (an extra `ls-remote` each run
when `origin/HEAD` was unset, two `rev-parse`s, a pre-push update-ref/reset and
a tree comparison). Five consecutive local shares: 117–136 ms (was 151–170 ms
here, 226–263 ms in the live omp run); `push` itself dominates.

## Review fixes, batches 1–2 (committed in 1cf10b44)

Batch 1 (real-use reports):
1. Doctor no longer counts missing context files as problems; it lists them as
   "not written yet (fine)". `am init` on an existing bank exits 0, says the bank
   exists and changes nothing. QA: doctor on a four-file-short bank, init twice.
2. Session-start text, activation block and SKILL.md now say saving needs a
   separate explicit yes, that agreeing a decision is not permission, to ask
   "Save this to project memory?", and give a valid `am remember --harness <client>
   --kind … --title … --body …` example. Hook text stays ~900 bytes plus summary.
3. `origin.harness` defaults to `unknown`; environment detection removed (env
   variables leak into nested harnesses). The hook example passes the observed
   client via `--harness`.
4. `am <command> --help` / `-h` (and `am help <command>`) print that command's
   usage and exit 0; argument errors point to it.
5. (Superseded by batch 3, item 1.) Interrupted-install cleanup; uninstall
   deletes block-only and empty hook files; install no longer counts its own
   leftover hook group as the file's original shape.
6. Share/refresh git calls cut as listed above; timings above.

Batch 2 (independent review):
1. `Scratch::new` (downloads, signature checks) again requires every ancestor of
   the spelled and physical `$TMPDIR`/`/tmp` to be root/user-owned and not
   writable by others unless sticky — install.sh's rule.
2. Every git call runs with `GIT_LITERAL_PATHSPECS=1`; QA shares a bank named
   `:` next to modified and new code files and only `:/…` lands.
3. Refresh refuses when the bank root or any component of a written path is a
   symlink (remember checks `topics/` too); bank reads skip links and
   non-regular files. QA: `topics -> ../src` refused, `src/` untouched.
4. `git merge-file` codes 1–127 are conflicts; anything else (binary files) aborts
   with nothing written. All merges are computed before any file is written.
   Checked ad hoc: a binary-file clash left the local bytes intact.
5. Refresh and share take the bank lock `remember` uses for the merge and
   snapshot, and re-check each file (MEMORY.md included) before replacing it.
6. Before staging, `git status --porcelain=v2` finds index entries that differ
   from HEAD and the working tree; unless that blob is in the recorded or the
   new shared commit, the command refuses. QA: refused, staged copy intact.
7. Hook/recall reads are bounded (1 MiB per file, regular files only); titles
   clipped; hook output capped at 8 KiB. Ad hoc: a 300 KB brief and a linked
   `techContext.md -> /etc/passwd` gave 2.4 KB of context without the link.
8. Bootstrap install refuses a release older than the installed version (v0.0.14
   receipts keep the version under `source`). Not exercised: it needs a signed
   release fixture.
9. MEMORY.md merges per row against the base: one-sided changes win, both-sided
   different changes keep yours and say so. QA: B took A's row edit; ad hoc:
   conflicting edits kept B's row with the message.
10. A title is stripped only when the body's first line is `# …`.
11. Prior receipt targets match through resolved parents, so a takeover with
   `~/.claude -> dotfiles/claude` works (QA fixture).
12. Linked `settings.json`, `hooks.json` and guidance files are edited at their
   targets and stay links (QA); a linked file is never deleted on release.
13. QA steps added for each of the above; the v0.0.14 takeover runs offline from
   a small generated fixture (receipt-v1, Python wrappers, skill link, activation
   block, prompt/Stop entries, source tree) with no product overrides.

All items were done with small changes; none needed new machinery. Batch-2 item 1
is exercised only on the signed-release path (downloads and signature checks),
which dev-source QA does not reach; macOS `$TMPDIR` and `/tmp` both pass the rule
(checked with `stat -L`).

## Review fixes, batch 3 (committed in 29e32aea)

1. **Leftovers only by proof.** The name-pattern sweep is gone. Before creating a
   temporary, moving a directory aside or creating a file, install/uninstall
   append it to `$XDG_STATE_HOME/agent-memory/install/leftovers` (fsynced). The
   next run removes only recorded temporaries, puts recorded moved-aside
   directories back (or drops them once the original is in place), and treats
   recorded created files as created by the installer; the record is deleted on
   commit or rollback. Unrecorded receipt temporaries and `.staging-*` roots are
   swept only inside am's own state/data directories. `am doctor` reports any
   unrecorded `.<name>.am-*` file beside a target as left in place, and a pending
   record. `--dry-run` deletes nothing (staging sweep skipped too). QA: a
   look-alike user file survives dry-run, install and uninstall and doctor names
   it; a recorded temporary is removed; dry-run leaves both and the record.
2. **Staging after push.** After a successful push `share` re-takes the bank lock
   and re-runs the staged-only check right before the index reset. If the user
   staged something meanwhile, the reset (and base update) is skipped and the
   output says the share succeeded but was not staged. Ad hoc: a pre-receive hook
   slept 1 s while a background job staged `progress.md`; the share reported it
   and the staged copy survived.
3. **Whole-file deletes only for files am created.** Activation files and hook
   configs are deleted on release only when the receipt says `originally_absent`
   (from the install that created them, a recorded interrupted install, or a
   v0.0.14 receipt) and nothing else is left; otherwise only our block/entries are
   removed. QA: a pre-existing `{"permissions":{"allow":[]}}` hooks.json and a
   whitespace-only AGENTS.md survive uninstall.
4. **MEMORY.md text merge.** Besides rows, the router's frontmatter and text
   around the table merge three-way (`git merge-file` on the text with rows
   replaced by one marker and the router time blanked). A both-sided text clash
   keeps yours and says so. Ad hoc: A's intro-line edit reached B while B had a
   local row.
5. **Retry base.** After a rejected push, the shared commit just merged is the
   base for the retry (and is accepted as a known staged blob). Ad hoc: a
   pre-receive hook moved `main` to a teammate's commit and rejected the first
   push; the retry landed on top of it with both notes.
6. **`.github/workflows/verify-release.yml`** (`workflow_dispatch`,
   `release: published`; macos-15, ubuntu-24.04, ubuntu-24.04-arm): the one-line
   installer from `main` in a temp HOME, `am --version` = release tag, `am status`,
   `am update` ("up to date"), init/remember/recall in a temp repo, `am uninstall`,
   then fails if any file or link is left in HOME (besides `.gitconfig`); per-step
   timings go to the step summary. Not run yet: it needs a published release.

All six were small changes. Item 1 uses the small journal file the review
suggested, because a temporary's content cannot prove it is ours.

## Review fixes, batch 4 (committed in c557117b)

1. **Leftovers record validated.** am acts on the record only if it is a regular
   file owned by the user; otherwise it does nothing with it, install refuses
   with a plain message, and doctor points at it. Each entry is checked before it
   is acted on: a `temp` must have the generated `.<target>.am-<uuid>` name, be a
   regular file, and sit beside a known target of this install/uninstall (links
   resolved) or in am's state directory; a `created` must be a known target; an
   `aside` must be a generated `.<name>.am-old-<uuid>` directory beside its
   original inside am's data or state parent. Anything else is left alone and
   appended to `leftovers.unrecognized`, which doctor lists for the user to inspect.
   QA: a recorded user file outside every target directory survives install and
   uninstall and doctor lists it.
2. **`created` recorded after the write.** A crash between the write and the note
   leaves the file unrecorded, so it is treated as the user's.
3. **One locked staging helper.** `stage_shared` takes the bank lock, re-runs the
   staged-only check and resets the index only if nothing would be lost; both the
   after-push path and the nothing-new path use it, and both say so when staging
   is skipped. (The nothing-new race was checked by reading the code; the push
   path's ad-hoc test from batch 3 still holds.)
4. **Moved-aside targets recovered first.** `install`, `update` and `uninstall`
   (not dry-run, not `update --check`) restore or drop recorded asides under the
   install lock before reading the source or preflighting targets. QA: a
   recorded `.runtime.am-old-<uuid>` is back at `runtime/` after install.
5. **Router time blanked only in the frontmatter.** A `router-updated:` line in
   the body merges as ordinary text (checked ad hoc: a body line of that form
   reached the other clone intact).

## Review fix, batch 5 (not committed)

- **Leftovers prove their bytes.** `temp` and `created` entries now record the
  SHA-256 of the bytes am writes (temp before the write, created after it). A
  temporary is deleted, and a file counts as created by am, only if its current
  hash matches; otherwise it is left as the user's and listed by doctor via
  `leftovers.unrecognized`. A partial write does not match, so it is kept and
  reported. Entries without a hash (older format) are treated the same way. QA: a
  recorded temporary path now holding a user's different file survives install
  and uninstall, and doctor lists it; the matching recorded temporary is removed.
  The receipt needs no hash: uninstall deletes a whole file only when it was
  created by am (`originally_absent`) **and** nothing but am's content is left
  (activation file whitespace-only after removing the block; hook config holding
  only empty objects/arrays after removing our entries). Any user content keeps
  the file, with only am's block or entries removed.

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
  plan (links, the omp extension, activation blocks, hook entries) and removes
  am temporaries and staging roots, but refuses a half-moved runtime directory
  until it is removed.
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
- **Superseded docs.** `docs/contracts/` and the design docs for cut v0.0.15
  machinery were deleted (2026-11-15 docs pass). ADRs stay, with an authority
  notice in `docs/adr/README.md` and "superseded" notes on ADR-0009, 0020, 0021,
  0026 and the ADR-0010 layout-recovery amendment; `docs/research/` and older
  release notes remain as history.
