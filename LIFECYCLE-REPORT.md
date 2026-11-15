# Lean am lifecycle checks

Real binaries, disposable homes and repositories, 2026-11-15. macOS arm64 used `/tmp/lean/release/am`; Linux arm64 used the static musl binary in Lima `am-rust-v015-linux-01`. Installs used development source; the v0.0.14 takeover started with the published, verified Python release, `install.sh --mode copy --presets claude,codex`. omp was not included in the legacy install. No product source was edited and no unit tests were written.

## Results by platform

| Check | macOS arm64 | Linux arm64 |
| --- | --- | --- |
| Published v0.0.14 takeover | **Fail:** status healthy, but doctor rejects the unchanged bank created by v0.0.14; details below | **Fail:** same four doctor findings |
| Existing repository bank | Pass: recall, query, status and doctor all exit 0, no problems | Pass: same |
| Interrupted development install | Partial pass: retry succeeds; abandoned temporary files remain | Partial pass: retry succeeds; abandoned temporary file and an empty activation file remain after uninstall |
| 2,000-note bank, 20,000 files | Pass timings | Pass timings |
| Odd repositories | Pass for the commands exercised | Pass for the commands exercised |

### Takeover details

Both platforms installed published v0.0.14, created developer and project identities using its preview/apply commands, and initialized a bank using its `init inspect`, `init preview`, and `init apply` workflow. The old bank contained `MEMORY.md`, `projectbrief.md`, and `activeContext.md`, with two routes and no lessons. The lean install took over receipt-v1 successfully; recursive `diff` of the bank against its pre-takeover copy was empty. Recall printed the legacy project brief and active context. The old managed Python source directory was removed; Claude and Codex hook commands changed from Python scripts to the new `am hook session-start --client ...` commands. Status reported no problems. User-supplied Claude `model` and custom setting and Codex custom setting survived installation; after `am uninstall`, both JSON files matched their pre-v0.0.14 content semantically (`jq -S` comparison). Legacy backup files remain, as described in the rebuild report. JSON byte formatting was not required for this comparison.

**Defect: valid v0.0.14 bank fails doctor and its suggested repair refuses.** On each platform, after the setup above:

```sh
am install --development-source "$LEAN_SOURCE" --presets claude,codex
cd "$LEGACY_REPO"
am status
am doctor
am init
```

`status` exits 0 and says `No problems found.` `doctor` exits 1 and says:

```text
Problems (4):
  - Context file productContext.md is missing; `am init` creates the six context files.
  - Context file systemPatterns.md is missing; `am init` creates the six context files.
  - Context file techContext.md is missing; `am init` creates the six context files.
  - Context file progress.md is missing; `am init` creates the six context files.
```

Following that advice on macOS exits 1: `am: /private/tmp/am-legacy-mac2/repo/memory-bank/MEMORY.md already exists; this project already has a memory bank.` Doctor should accept supported existing banks or provide a workable, non-destructive remedy. This is a failed takeover pass criterion, not a failure to preserve the bank.

### Existing real bank

Copied this worktree's files and bank to disposable directories, excluding `.git` and build `target`, then initialized a fresh Git repository in each copy. Doctor found no problems; it reported 20 routes and 7 active lessons. Recall printed the actual project brief and technical context; `recall install` also returned an install-related lesson. Output was sensible, although the historical technical-context file still references older Python-era files, as its source text does. Times are individual wall-clock samples in milliseconds, output redirected to a file:

| Command | macOS | Linux |
| --- | ---: | ---: |
| `am recall` | 8.2 | 1.4 |
| `am recall install` | 8.4 | 0.9 |
| `am status` | 8.0 | 4.3 |
| `am doctor` | 8.2 | 0.8 |

An initial Linux copy accidentally included macOS AppleDouble `._*.md` transport artifacts and doctor correctly flagged seven as unreadable UTF-8; these were not part of the original bank. The table uses a clean transfer. An initial run also omitted the disposable `~/.local/bin` from PATH, and doctor correctly flagged that setup mistake; the table uses the installed command on PATH.

### Interrupted install

For each platform and each delay `0.001`, `0.01`, `0.04`, `0.10` seconds, started a development-source install into a new disposable HOME with Claude and Codex presets, sent SIGKILL after the delay, then reran the same install. All retries exited 0. At 1 ms both platforms were interrupted before receipt creation and left a skill link, lock, and an in-progress hidden atomic-write file; Linux had also created `CLAUDE.md`. At the longer delays an installation had completed or left more targets; the receipt was present at 40 and 100 ms on macOS and 10, 40 and 100 ms on Linux. This exercised the development-link path, not a signed-release runtime-directory move; timing cannot guarantee which instruction the signal interrupts.

**Defect: successful recovery and uninstall leave interrupted atomic-write files.** Reproduction, with a fresh disposable HOME and a valid lean source:

```sh
mkdir -p "$HOME/.claude"
am install --development-source "$LEAN_SOURCE" --presets claude,codex &
pid=$!; sleep 0.001; kill -9 "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
am install --development-source "$LEAN_SOURCE" --presets claude,codex
am uninstall
find "$HOME" \( -type f -o -type l \) -print
```

On macOS the final listing still contained `.claude/.CLAUDE.md.am-789f104b-e385-4207-9811-c31f155c840b`. On Linux it contained `.claude/.settings.json.am-5b80f8d8-72a2-4faa-94e9-d65abf755195` and an empty `.claude/CLAUDE.md`, which had not existed before the interrupted install. UUIDs and outcome depend on timing. The user can rerun install normally; to return these observed homes to their pre-install state after uninstall, manually remove the identified orphan temporary file and, on Linux, the empty `CLAUDE.md` after checking it contains no user content. Do not delete similarly named files without inspecting them.

### Scale

A fresh bank was populated directly with 2,000 distinct Markdown notes and 2,000 route rows; a separate directory in the repository contained 20,000 empty files. Status counted 2,001 active lessons after the measured `remember`. Single warm wall-clock samples below include process startup and redirected output. A PATH shim logged and executed the real `git`: every measured command invoked git exactly once. Status in these measurements flagged that the disposable installed bin directory was absent from PATH, a harness setup issue; the command still read the bank and exited 0. The other reported measurements do not depend on the PATH diagnostic.

| Command | macOS ms | macOS git calls | Linux ms | Linux git calls | Target |
| --- | ---: | ---: | ---: | ---: | --- |
| `am recall` | 36.6 | 1 | 10.2 | 1 | <200 ms |
| `am recall word` | 42.3 | 1 | 16.9 | 1 | <200 ms |
| `am remember --kind fact --title 'Scale unique note' --body 'Unique scale note body'` | 46.8 | 1 | 22.3 | 1 | <200 ms |
| `am status` | 40.1 | 1 | 12.5 | 1 | <200 ms |
| `am hook session-start --client claude` | 34.4 | 1 | 9.6 | 1 | <100 ms |

### Odd repositories

On each platform, in a fresh Git repository with neither a remote nor configured `user.email` (global and system Git configuration disabled), `am init` and `am status` exited 0. The repository path contained spaces and `ü`; `am recall` from its subdirectory exited 0. After `git config agent-memory.bankroot 'custom bank'`, `am init` created `custom bank/` and `am recall` reported that bank. After committing and creating a linked Git worktree, `am recall` in the worktree also found `custom bank/` and exited 0. Status's unrelated PATH warning in this particular run is described in the scale section. Git author environment variables were supplied solely to create the worktree's initial commit; no `user.email` configuration was set.
