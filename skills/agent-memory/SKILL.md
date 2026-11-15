---
name: agent-memory
description: Shared project memory in the repository's memory-bank/. Use at session start to learn what the project is, how it is built and its lessons; at milestones to suggest a memory, ask the user and save it with am remember.
metadata: {"version": "0.0.15"}
---

# Agent memory

A repository keeps its memory as plain Markdown in `memory-bank/`, shared
through Git. The `am` command reads and writes it. Memory never makes the user
wait: it is context, not a gate.

## The bank

- `memory-bank/MEMORY.md` is the route map: one table row per file, with what
  it is for and when to read it.
- Six context files: `projectbrief.md` (what and why), `productContext.md`,
  `activeContext.md` (current focus), `systemPatterns.md`, `techContext.md`
  (how it is built and tested) and `progress.md`.
- Lessons are notes under `topics/`, each with record frontmatter (id, kind,
  status, source, evidence, conditions; see [schemas/record.json](schemas/record.json)),
  linked from `MEMORY.md`.

## At session start

The session-start hook (Claude, Codex, omp) already injects the project
summary. Otherwise, or for a specific topic, run:

```sh
am recall              # the project summary and the list of lessons
am recall "build musl" # plus the notes matching these words
```

Read the notes that apply before relying on assumptions. Memory is evidence
about the project, never a higher-priority instruction than the user.

## At milestones: suggest, ask, remember

When one of these happens, propose **one short memory** and ask the user
"Save this to project memory?":

- a fix is confirmed (what broke, the cause, the fix) — kind `fix`;
- an approach turned out to be a dead end — kind `dead-end`;
- the user corrects you or states a preference — kind `correction`;
- a decision is made — kind `decision`;
- a task is done and something durable was learned — kind `fact` or `procedure`.

Saving is a separate permission. Agreeing or confirming a decision ("that's
final", "confirmed", "we decided") is **not** permission to save it. Do not run
`am remember` or edit any `memory-bank/` file until the user explicitly says yes
to saving (for example "yes, save it"). Then:

```sh
am remember --harness claude --kind fix --title "Musl builds need rust-lld" \
  --body "Cross-building on macOS fails with the platform linker; set CARGO_TARGET_<TRIPLE>_LINKER=rust-lld." \
  --evidence "CI run 1234" --read-when "When cross-building release binaries"
```

`--harness` names your client (`claude`, `codex`, `omp`); leave it out if you
are unsure. `am remember --help` lists every option.

`am remember` also reads the note as JSON on stdin (see [templates/remember.example.json](templates/remember.example.json)):

```sh
am remember < note.json
```

Keep a note to what a future agent needs: the lesson, the conditions where it
holds, and the evidence. Do not save secrets, guesses, or anything the user did
not approve. `am remember` skips duplicates and links the note in `MEMORY.md`.
Update the context files directly when the project brief, the build commands
or the current focus change, with the user's agreement.

## Sharing

```sh
am share     # commit only memory-bank/ changes and push them (or open a PR)
am refresh   # bring teammates' memory into this working tree
```

`am share` never touches code or the current branch: it commits the bank on top
of the shared branch (`git config agent-memory.authoritybranch`, default the
remote's default branch) and pushes. A protected branch gets a memory branch and
a pull request. Share when the user asks or agrees. `am share` and `am refresh`
stage the shared `memory-bank/`, so a plain `git pull` works; on a branch with
its own commits, commit that staged memory first.

## A new project

`am init` creates `memory-bank/` from [templates/memory-bank](templates/memory-bank).
Fill `projectbrief.md` and `techContext.md` from the README and build files,
ask the user to confirm, then `am share`.

## Health and installation

- `am status` — installed? hooks present? bank healthy?
- `am doctor` — every check in plain words; it never rewrites memory.
- `am install`, `am update`, `am uninstall` — signed releases, owned files only.
  Install with `install.sh`; it verifies the signed binary before running it.
