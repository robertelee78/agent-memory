# Lean am lifecycle checks

Real binaries, disposable homes and repositories, 2026-11-15. macOS arm64 used `/tmp/lean/release/am`; Linux arm64 used the static musl binary in Lima `am-rust-v015-linux-01`. Installs used development source; the v0.0.14 takeover started with the published, verified Python release, `install.sh --mode copy --presets claude,codex`. omp was not included in the legacy install. No product source was edited and no unit tests were written.

## Results by platform

| Check | macOS arm64 | Linux arm64 |
| --- | --- | --- |
| Published v0.0.14 takeover | Pass on recheck: status, doctor, recall and harmless init | Pass on recheck: same |
| Existing repository bank | Pass: recall, query, status and doctor all exit 0, no problems | Pass: same |
| Interrupted development install | Pass on recheck: retry, doctor and uninstall succeed; no files or links left | Pass on recheck: same, with one observed SIGKILL before completion |
| Takeover with symlinked `~/.claude` | Pass | Pass |
| v0.0.14 updater against lean release-v2 | Pass in macOS simulation: clean refusal, unchanged disposable home | Not separately simulated |
| 2,000-note bank, 20,000 files | Pass timings | Pass timings |
| Odd repositories | Pass for the commands exercised | Pass for the commands exercised |

## Recheck after fixes

Rebuilt both binaries from the current tree using `CARGO_TARGET_DIR=/tmp/lean cargo build --release` and the specified `aarch64-unknown-linux-musl` build with `rust-lld`. Development installs used disposable copies of the current skill pack with the rebuilt binary in `target/release/am`.

On both platforms, repeated the published v0.0.14 install and old-runtime sparse-bank initialization, takeover, status, doctor, recall, init, byte comparison and uninstall. Both status and doctor exited 0 with `No problems found.` `am init` exited 0 and printed:

```text
This project already has a memory bank at memory-bank/; nothing was changed. Context files it lacks are optional: add them when there is something useful to say.
```

Recursive bank comparison was empty after init. Recall read the existing bank; the old Python managed source directory was gone; Claude and Codex hooks targeted the rebuilt lean binary. The original user JSON settings were present while installed and semantically equal to their pre-agent-memory content after uninstall.

Repeated the same complete takeover on both platforms with `~/.claude` a symlink to another disposable directory before installing v0.0.14. The legacy install and lean takeover succeeded; doctor, status, recall and init passed; the bank comparison was empty; and Claude/Codex settings were restored after uninstall. The Claude directory remained a symlink pointing at the original directory.

Repeated SIGKILL at 1, 3, 6, 10, 20 and 40 ms in fresh homes, then reran install, ran doctor and uninstalled. On macOS, five kills landed during install (exit 137), including interrupted activation, settings, and receipt writes; at 40 ms install had already completed. On Linux, the 1 ms kill landed after activation and settings appeared but before the receipt; the other five installs completed before the signal. Every retry, doctor and uninstall exited 0, and `find` found no files or symlinks under any of the homes after uninstall. Empty directories were not compared. This covers development-source installs, not a killed signed-release runtime move; as always, exact crash points depend on scheduling. For the observed cases a user needs only to rerun install, then uninstall if desired; no manual file removal was necessary.

## Old updater against a lean release

Inspected the published `v0.0.14` `skills/agent-memory/scripts/agent_memory/release_distribution.py` and `updating.py`. The old updater downloads the GitHub API `/releases/latest` locator (maximum 65,536 bytes), then `release-manifest.json` (65,536 bytes) and `release-manifest.json.sshsig` (4,096 bytes). It verifies the SSH signature with principal `release@agent-memory`, namespace `agent-memory.release`, and a public key pinned in the Python source before parsing the manifest. Its exact nine-field manifest allowlist is checked before requiring schema `agent-memory/release-v1`; only after this authentication does it download `agent-memory.tar.gz` bounded by the verified archive size and its 17,825,792-byte maximum. Download transport requires HTTPS. Thus a properly signed lean release-v2 containing additional fields is rejected at the field check, before schema or archive acquisition.

Built an actual throwaway-signed release with the current lean `am-dev release build`, in an isolated scratch copy with a throwaway public key substituted for the pinned signing key. A temporary `git` shim supplied that same scratch key for the tracked `allowed-signers` blob, so the generated pack is test-only and is not a release artifact. The builder emitted `install.sh`, `agent-memory.tar.gz`, `release-manifest.json`, its `.sshsig`, and `am-<triple>` plus `.sshsig` for each of `aarch64-apple-darwin`, `aarch64-unknown-linux-musl`, and `x86_64-unknown-linux-musl`. The manifest was **1,153 bytes**, schema `agent-memory/release-v2`, and has the extra `targets` and `command_surface` fields. It fits the old 65,536-byte limit.

For the real updater simulation, installed published v0.0.14 in another disposable macOS HOME. The old updater embeds its public key in Python, so changing only its `allowed-signers` file cannot redirect the probe's trust: in this disposable copy, changed its pinned public-key constant to the throwaway key. A PATH `curl` shim returned a local stable-release locator, the actual generated manifest and its actual signature for the HTTPS URLs, without making a network request. This changes the transport and trust root for the experiment; the manifest parsing and signature verification were the old runtime's real code. After those setup changes, copied the entire HOME as the baseline, ran `am update`, and compared the entire HOME recursively against that baseline. The command exited 1, fetched exactly the locator, manifest and signature, did not request the archive, and `diff -r` produced no differences. The exact user-facing output was:

```text
am: Unknown release manifest fields
Preserve the active runtime and receipts. Check the trusted release, curl and OpenSSH signature support from an operator-trusted installer; never fall back to unsigned bytes.
```

This demonstrates a clean refusal and unchanged file contents for the simulated trust/transport setup; the text is generic and does not explain that the release format changed. The simulation was not repeated on Linux.

### Takeover details

Both platforms installed published v0.0.14, created developer and project identities using its preview/apply commands, and initialized a bank using its `init inspect`, `init preview`, and `init apply` workflow. The old bank contained `MEMORY.md`, `projectbrief.md`, and `activeContext.md`, with two routes and no lessons. The lean install took over receipt-v1 successfully; recursive `diff` of the bank against its pre-takeover copy was empty. Recall printed the legacy project brief and active context. The old managed Python source directory was removed; Claude and Codex hook commands changed from Python scripts to the new `am hook session-start --client ...` commands. Status reported no problems. User-supplied Claude `model` and custom setting and Codex custom setting survived installation; after `am uninstall`, both JSON files matched their pre-v0.0.14 content semantically (`jq -S` comparison). Legacy backup files remain, as described in the rebuild report. JSON byte formatting was not required for this comparison.

**Fixed on recheck: valid v0.0.14 bank previously failed doctor and its suggested repair refused.** Historical reproduction on each platform, before the fix:

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

Following that advice on macOS exited 1: `am: /private/tmp/am-legacy-mac2/repo/memory-bank/MEMORY.md already exists; this project already has a memory bank.` The later recheck below passed.

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

**Fixed on recheck: successful recovery and uninstall previously left interrupted atomic-write files.** Historical reproduction, with a fresh disposable HOME and a valid lean source:

```sh
mkdir -p "$HOME/.claude"
am install --development-source "$LEAN_SOURCE" --presets claude,codex &
pid=$!; sleep 0.001; kill -9 "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
am install --development-source "$LEAN_SOURCE" --presets claude,codex
am uninstall
find "$HOME" \( -type f -o -type l \) -print
```

Before the fix, on macOS the final listing still contained `.claude/.CLAUDE.md.am-789f104b-e385-4207-9811-c31f155c840b`. On Linux it contained `.claude/.settings.json.am-5b80f8d8-72a2-4faa-94e9-d65abf755195` and an empty `.claude/CLAUDE.md`, which had not existed before the interrupted install. UUIDs and outcome depend on timing. The later recheck below found no remaining files or links.

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
