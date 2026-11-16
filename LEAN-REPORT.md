# v0.0.15 lean `am` — rebuild report

Branch `lean/v0.0.15`, built against [docs/design/v0.0.15-lean.md](docs/design/v0.0.15-lean.md).
Committed through review batch 4 (c557117b); batch 5 below is not.

## v0.0.16 fix release (branch `fix/v0.0.16`, PR #271: 0d30d596, review fixes 833e1372; the upstream fix below not committed)

Three fixes ([#268](https://github.com/robertelee78/agent-memory/issues/268),
[#269](https://github.com/robertelee78/agent-memory/issues/269),
[#270](https://github.com/robertelee78/agent-memory/issues/270)), version 0.0.16
(`Cargo.toml`, `Cargo.lock`, SKILL.md metadata; `release::VERSION` follows Cargo).
Notes: [docs/releases/v0.0.16.md](docs/releases/v0.0.16.md).

### #268 omp set up by default

- omp 18.20.1 source (`packages/utils/src/dirs.ts`): the agent directory is a
  non-empty `PI_CODING_AGENT_DIR` (resolved), else `~/<PI_CONFIG_DIR or .omp>/agent`;
  `discovery/builtin.ts` loads `extensions/`, `skills/` and `AGENTS.md` from it.
  `am` resolves it the same way (named profiles, `OMP_PROFILE`, are not followed).
- `all` (the default) now includes omp when that directory exists, or when
  `--omp-agent-dir` is given or recorded. No omp: nothing omp-related is created.
  `--presets` without `omp` excludes it. A detected directory is not recorded,
  so it is re-resolved on every install/update.
- `am update` on an up-to-date release install now re-plans and activates when
  the plan differs from the receipt (omp installed since), instead of returning
  "up to date". `am status`/`am doctor` report "omp is installed (…) but
  agent-memory is not set up for it yet. Run `am update`."
- From v0.0.15, `am update` is run by the v0.0.15 binary, whose plan has no omp:
  it installs v0.0.16 without omp. `am status` then says so; a second `am update`
  (now v0.0.16) sets omp up. The one-line installer does it in one step.
  Checked end to end on the signed-release path, in scratch copies of d77f1feb
  and this branch with a throwaway key and `file://` asset URLs (test-only
  patch, not in the product), `~/.omp/agent/extensions/vox.ts` present:
  v0.0.15 `am install --bootstrap-binary` (no omp files); `am update` by
  v0.0.15 → "Installed agent-memory 0.0.16 (release)" (104 ms), still no omp;
  `am status` (v0.0.16) shows the omp problem; `am update` by v0.0.16 (61 ms)
  adds the omp skill link, AGENTS.md block and extension, `vox.ts` untouched;
  status "codex ok, claude ok, omp ok", no problems; a third `am update` says
  "up to date"; `am doctor` verifies the runtime; `am uninstall` leaves only
  `vox.ts`. `release.rs`, the manifest fields and the command surface are
  unchanged, so v0.0.15 accepts the v0.0.16 release.

### #269 install on a fresh machine and after `am uninstall`

- `read_receipt` treats a receipt marked `"uninstalled": true` (v0.0.14's
  uninstall keeps it) as no installation, for install, update, uninstall,
  status and doctor; install replaces it with a new receipt.
- The uncommitted preflight change is kept: a recorded target that no longer
  exists is installed again (a user deleting `~/.local/bin/am`, say). A file
  that agent-memory did not put at a target is still refused.
- Reproduced first: the qa fixture (shape of the operator's
  `receipt.v014.json`, generated in the script) makes v0.0.15 (d77f1feb) refuse
  with "… changed since agent-memory installed it" for each link; v0.0.16
  installs.

### #270 `git pull` after `am refresh` and `am share`

Experiment first (`git` 2.55, bare remote, a teammate clone and a user clone,
real `am` for refresh/share, candidates applied with git afterwards). 30 cells
(merge/rebase × up to date, behind with code commits, feature branch, more
memory after refresh (up to date / behind) × refresh/share, plus unshared
edits). Pull succeeded with readable notes in:

| Candidate | Cells passing | Trace / side effects |
| --- | ---: | --- |
| v0.0.15 (staged) | 9/30 | every rebase cell fails: "Your index contains uncommitted changes" |
| move branch over memory-only commits, else staged | 16/30 | none |
| + (a) memory-only commit when pull rebases | 20/30 | a commit; the rebasing pull drops it |
| + (b) repo-local `rebase.autoStash`/`merge.autoStash` | 22/30 | config changed in 18 cells; 6 more "pass" with conflict markers in MEMORY.md and a stash entry |

(b) only wins on cells where the user's own unstaged edits block a rebasing
pull, and leaves conflict markers when memory moved again; (a) never leaves a
mess. Chosen: move the branch when possible, else (a), extended to merging
pulls on a branch with its own commits (the old message already told users to
make exactly that commit; with it the merge finds identical files), else
staged for a fast-forward merge pull. A memory-only commit is made only when
the branch's own commits leave the bank alone (so it never reverts the user's
committed memory), and one made earlier (bank = the previous shared memory,
not on the upstream) is replaced, not stacked, so `am refresh` again fixes
"more memory shared before you pull". No config change.

Final matrix with the built `am`, after the review fixes: 144 cells
(`matrix3.sh`, real git and `am`, kept outside the repo). Pull modes: merge,
rebase, and each with `pull.ff=only`. Situations: up to date when memory
arrives; behind (code and memory upstream); diverged (own unpushed code
commit); feature branch level with its own upstream; feature branch whose
local and upstream (`origin/feat`) code commits diverged; branch without
upstream; more memory shared before the pull (up to date / behind); behind
and `am refresh` run again. Uncommitted: none, an unshared memory edit, or an
edit to a tracked code file. Each cell: what `am` did (moved / commit /
staged); then either "ok" (plain `git pull` works) or "→" the step `am`
printed, followed exactly, after which the pull works and the tree is clean.

| situation | cmd | uncommitted | merge | rebase | merge, ff=only | rebase, ff=only |
| --- | --- | --- | --- | --- | --- | --- |
| up | refresh | none | moved; ok | moved; ok | moved; ok | moved; ok |
| up | refresh | memory | moved; ok | moved; → am share, `git pull` | moved; ok | moved; → am share, `git pull` |
| up | refresh | code | moved; ok | moved; → stash, `git pull` | moved; ok | moved; → stash, `git pull` |
| up | share | none | moved; ok | moved; ok | moved; ok | moved; ok |
| behind | refresh | none | staged; ok | commit; ok | staged; ok | commit; → `git pull --rebase` |
| behind | refresh | memory | staged; ok | commit; → am share, `git pull` | staged; ok | commit; → am share, `git pull --rebase` |
| behind | refresh | code | staged; ok | commit; → stash, `git pull` | staged; ok | commit; → stash, `git pull --rebase` |
| behind | share | none | staged; ok | commit; ok | staged; ok | commit; → `git pull --rebase` |
| diverged | refresh | none | commit; ok | commit; ok | commit; → `git pull --no-ff` | commit; → `git pull --rebase` |
| diverged | refresh | memory | commit; ok | commit; → am share, `git pull` | commit; → am share, `git pull --no-ff` | commit; → am share, `git pull --rebase` |
| diverged | refresh | code | commit; ok | commit; → stash, `git pull` | commit; → stash, `git pull --no-ff` | commit; → stash, `git pull --rebase` |
| diverged | share | none | commit; ok | commit; ok | commit; → `git pull --no-ff` | commit; → `git pull --rebase` |
| feature | refresh | none | commit; ok | commit; ok | staged; ok | commit; ok |
| feature | refresh | memory | commit; ok | commit; → am share, `git pull` | staged; ok | commit; → am share, `git pull` |
| feature | refresh | code | commit; ok | commit; → stash, `git pull` | staged; ok | commit; → stash, `git pull` |
| feature | share | none | commit; ok | commit; ok | staged; ok | commit; ok |
| feature, diverged from its upstream | refresh | none | commit; ok | commit; ok | commit; → `git pull --no-ff` | commit; → `git pull --rebase` |
| feature, diverged from its upstream | refresh | memory | commit; ok | commit; → am share, `git pull` | commit; → am share, `git pull --no-ff` | commit; → am share, `git pull --rebase` |
| feature, diverged from its upstream | refresh | code | commit; ok | commit; → stash, `git pull` | commit; → stash, `git pull --no-ff` | commit; → stash, `git pull --rebase` |
| feature, diverged from its upstream | share | none | commit; ok | commit; ok | commit; → `git pull --no-ff` | commit; → `git pull --rebase` |
| no upstream | refresh | none | staged; says no upstream | staged; says no upstream | staged; says no upstream | staged; says no upstream |
| no upstream | refresh | memory | staged; says no upstream | staged; says no upstream | staged; says no upstream | staged; says no upstream |
| no upstream | refresh | code | staged; says no upstream | staged; says no upstream | staged; says no upstream | staged; says no upstream |
| no upstream | share | none | staged; says no upstream | staged; says no upstream | staged; says no upstream | staged; says no upstream |
| up, more memory | refresh | none | moved; ok | moved; ok | moved; ok | moved; ok |
| up, more memory | refresh | memory | moved; ok | moved; → am share, `git pull` | moved; ok | moved; → am share, `git pull` |
| up, more memory | refresh | code | moved; ok | moved; → stash, `git pull` | moved; ok | moved; → stash, `git pull` |
| up, more memory | share | none | moved; ok | moved; ok | moved; ok | moved; ok |
| behind, more memory | refresh | none | staged; → am refresh, `git pull` | commit; → am refresh, `git pull` | staged; → am refresh, `git pull` | commit; → am refresh, `git pull --rebase` |
| behind, more memory | refresh | memory | staged; → am refresh, am share, `git pull` | commit; → am refresh, am share, `git pull` | staged; → am refresh, am share, `git pull` | commit; → am refresh, am share, `git pull --rebase` |
| behind, more memory | refresh | code | staged; → am refresh, stash, `git pull` | commit; → am refresh, stash, `git pull` | staged; → am refresh, stash, `git pull` | commit; → am refresh, stash, `git pull --rebase` |
| behind, more memory | share | none | staged; → am refresh, `git pull` | commit; → am refresh, `git pull` | staged; → am refresh, `git pull` | commit; → am refresh, `git pull --rebase` |
| behind, refresh again | refresh | none | staged; ok | commit; ok | staged; ok | commit; → `git pull --rebase` |
| behind, refresh again | refresh | memory | staged; ok | commit; → am share, `git pull` | staged; ok | commit; → am share, `git pull --rebase` |
| behind, refresh again | refresh | code | staged; ok | commit; → stash, `git pull` | staged; ok | commit; → stash, `git pull --rebase` |
| behind, refresh again | share | none | staged; ok | commit; ok | staged; ok | commit; → `git pull --rebase` |

68 of 144 cells pull with a plain `git pull`. In the other 76 the pull is
refused, but only where the printed message says so, and following its step
works: "If more memory is shared before you pull, run am refresh first";
"You have uncommitted changes[ (memory edits not shared yet: am share shares
them)]; git will ask you to commit or stash them before a rebasing pull"
(a rebasing pull refuses any uncommitted tracked change, with or without `am`);
"Pull with `git pull --rebase`" / "`git pull --no-ff`: with pull.ff=only a
plain `git pull` refuses, as this branch and its upstream have both moved";
"This branch has no upstream branch, so there is nothing for `git pull` to
pull". No cell lost an edit. Before following the step, notes are readable in
every cell except the two rebase-mode "behind, more memory" cells, where the
pull stops on a `MEMORY.md` conflict (`am refresh` beforehand avoids it; `git
rebase --skip` recovers).

Code: `share.rs` `settle` (after `record_shared`, under the bank lock),
`settle_note`, `pull_rebases` (`branch.<b>.rebase`, else `pull.rebase`),
`temp_index` shared with `commit_bank`. An up-to-date refresh (or "nothing new"
share) also settles memory left staged by v0.0.15 when the index already holds
the shared bank. Extra git calls: about 4 to move the branch, about 10 for a
memory-only commit, one `git status` in rebase mode.

### Review fixes (PR #271, not committed)

1. **Runtime never adopted.** The "already up to date, set up what is new"
   path re-fingerprinted the runtime, so a file the user put in `runtime/`
   became owned and uninstall deleted it. A kept runtime (`Want::Runtime(None)`)
   now keeps the previous receipt's identity. `am doctor` names the true fix
   for a changed runtime ("Move out any file you added there, or move the
   directory away and re-run install.sh; until then uninstall leaves it in
   place"); its old advice, `am install`, could not repair it. Hand test on
   the signed-release path (throwaway key, `file://` URLs, scratch copies;
   v0.0.15 install → update by v0.0.15 → `my-notes.txt` put in `runtime/` →
   update by v0.0.16 adding omp → update again → doctor → uninstall): runtime
   identity in the receipt unchanged by the update; uninstall says "Left in
   place … runtime" and `my-notes.txt` survives. The same test on 0d30d596:
   identity changed and the file was **gone** after uninstall. Both doctor
   remedies verified: moving the file out gives "No problems found"; moving the
   runtime away and re-running the bootstrap installs a fresh runtime (no
   problems) and keeps the moved directory.
2. **QA isolation.** `scripts/qa.sh` unsets `PI_CODING_AGENT_DIR` and
   `PI_CONFIG_DIR` next to the other inherited locations; the scenarios that
   need them pass them with `env`. `verify-release.yml` unsets both after
   creating its temporary HOME.
3. **`pull.ff=only`.** Tested (`ffonly.sh`): with `pull.rebase=true` and
   `pull.ff=only`, behind with code commits, a memory commit followed by
   `git pull --rebase` ends clean at the upstream; leaving the memory staged
   makes every pull form refuse (`--rebase`, `--ff-only` and plain). So the
   commit stays and `am` says to use `git pull --rebase`.
   **Second review (not committed):** the decision and the message used
   whether the branch tracks the shared branch, not the branch's own pull
   upstream. Now both use `@{upstream}`: with a merging pull and
   `pull.ff=only` the memory stays staged while HEAD can still fast-forward to
   its upstream (`merge-base --is-ancestor HEAD @{upstream}`), else `am`
   commits it; and after settling, `am` names `git pull --rebase` /
   `git pull --no-ff` only when HEAD (with any memory commit) and its upstream
   have both moved (`rev-list --left-right --count HEAD...@{upstream}`, both
   counts above zero). The reviewer's example (feature branch tracking
   `origin/feature`, `pull.rebase=false`, `pull.ff=only`, local and upstream
   code commits diverged) now gets a memory commit and "Pull with `git pull
   --no-ff`"; a plain pull refuses as said and `git pull --no-ff` ends clean.
   The other direction: a feature branch level with its upstream, rebase and
   `pull.ff=only`, gets the memory commit and "`git pull` works as usual", and
   a plain pull does work (before, `am` wrongly asked for `--rebase` there).
   `@{upstream}` is the remote-tracking ref as last fetched; `am` fetches only
   the shared branch, so a feature upstream that moved since the user's last
   fetch is judged on what was fetched.
4. **Uncommitted changes.** In rebase mode `am` runs one `git status
   --porcelain --untracked-files=no` and, when anything tracked is changed or
   staged, says "You have uncommitted changes; git will ask you to commit or
   stash them before a rebasing pull." (with the `am share` hint when memory
   edits are among them) instead of "works as usual". It touches nothing.
5. **Relative `PI_CODING_AGENT_DIR`.** Resolved against the current directory
   with `..` folded, like omp's `path.resolve`; detection returns nothing
   rather than failing, so an install whose presets leave omp out can never be
   blocked by it. `--omp-agent-dir` keeps the strict absolute rule.

Also: a branch without an upstream gets "This branch has no upstream branch,
so there is nothing for `git pull` to pull." and the memory stays staged (no
commit is made for a pull that cannot happen).

### Checks

- `cargo build --release`: 0 warnings, also with `--features dev-tools`
  (rebuilt after `cargo clean`); `cargo fmt --check` clean;
  `aarch64-unknown-linux-musl` (statically linked) and `x86_64-unknown-linux-musl`
  (static-pie) with `rust-lld`: 0 warnings.
- `scripts/qa.sh` on macOS arm64: **0 failures, 85 steps + 95 checks** (v0.0.15:
  40 + 51). New since v0.0.15: plain install with omp present beside the user's
  `vox.ts`; `--presets` without omp; status lists omp; pull-mode section (R
  rebases, M merges; refresh and share each, up to date, behind, refresh
  twice, unshared edits); diverged clone C gets the memory-only commit; fresh
  machine, reinstall after the lean uninstall, omp installed later + `am
  update`, `PI_CODING_AGENT_DIR`; the v0.0.14-uninstalled fixture, including the
  refusal of a user's file at a target. Added with the review fixes: rebase +
  `pull.ff=only` (message, plain pull refused as said, `git pull --rebase`
  clean), merge + `pull.ff=only` (staged, plain pull clean), an uncommitted
  code edit in rebase mode (message, plain pull refused as said, stash/pull/pop
  keeps the edit), a branch without upstream, a relative `../shared-agent`
  `PI_CODING_AGENT_DIR` (resolved like omp) and one that cannot resolve with
  omp left out of the presets. Added with the second review: the feature
  branch diverged from `origin/feature` (merge, `pull.ff=only`: message names
  `git pull --no-ff`, plain pull refused as said, `--no-ff` clean with the
  upstream merged and the note readable) and a feature branch level with its
  upstream (rebase, `pull.ff=only`: memory commit, "works as usual", plain
  pull clean). Finding 1 has no development-install equivalent (a development
  install records no runtime), hence the signed hand test above.
- Timings (ms), last run: refresh 63–167, share 55–217 (local push included;
  217 is share behind code in rebase mode, with the memory commit), `git pull`
  29–52, install 66–79 (first cold `--dry-run` 215), update adding omp 31,
  uninstall 3–28, recall/remember/status/doctor/hook 0–15.

### Not done

- Linux qa runs here (no Linux host); Main reports CI and Linux arm64 QA green
  for 833e1372, not yet for the upstream fix.
- `am update` v0.0.15 → v0.0.16 against the real GitHub release (needs the
  published release and the real key; done above with a throwaway key).
- `am uninstall` leaves the empty `~/.omp/agent/skills/` it created (as the
  v0.0.15 omp preset did).

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
