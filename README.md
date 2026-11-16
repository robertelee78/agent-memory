# agent-memory

Shared project memory for coding agents, as plain Markdown in your repository.

A fresh agent entering a project quickly learns what the project is and why,
how it is built, and the lessons: fixes, dead ends, your corrections and your
decisions. Memory lives in the repository's `memory-bank/` and travels through
Git. It never makes you wait.

The design is [docs/design/v0.0.15-lean.md](docs/design/v0.0.15-lean.md).

## Install

macOS (Apple silicon) and Linux (x86_64, arm64):

```sh
curl -fsSL https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh | sh
```

The bootstrap needs `curl` and OpenSSH `ssh-keygen`. It downloads the `am`
binary for your host, verifies its signature against the release key pinned in
the script, and only then runs `am install`. The verified binary checks the
signed release manifest and skill pack, installs itself under
`~/.local/share/agent-memory/runtime`, links `am` and `agent-memory` into
`~/.local/bin`, and records every file it owns in a receipt
(`~/.local/state/agent-memory/install/receipt.json`).

Defaults cover the Agent Skills directory, Codex, Claude and OpenCode, plus omp
when it is on the machine (its agent directory, `$PI_CODING_AGENT_DIR` or
`~/.omp/agent`, exists):

- the skill (`skills/agent-memory`) is linked into each client's skills folder;
- a short activation block is added to `AGENTS.md` / `CLAUDE.md`;
- Claude and Codex get a **session-start hook** that injects the project summary;
  omp gets a small extension (`extensions/agent-memory.ts`) that hands the
  summary to the first prompt of each session. Your other omp files stay as they are.

Options (pass after `sh -s --`): `--dry-run`, `--presets all|none|agents,codex,claude,opencode,omp`,
`--mode link|copy`, `--target-dir PATH`, `--bin-dir PATH`, `--no-guidance`,
`--omp-agent-dir PATH` (set up omp in another agent directory). If you install
omp later, `am update` sets it up.

Restart running agent sessions afterwards. Then:

```sh
am status      # installed? hooks present? bank healthy?
am update      # newest signed release
am uninstall   # remove exactly what the receipt owns
```

An existing v0.0.14 (or older) installation is taken over by re-running the
one-line install: each recorded file is replaced only if it is unchanged since
it was installed, and the old per-turn hooks are removed. After `am uninstall`
(of any version) the one-line install works like a fresh one. A file that
agent-memory did not put at an install location is never overwritten: the
install stops and names it.

## Use

```sh
am init                     # create memory-bank/ in this repository
am recall                   # the project summary and the list of lessons
am recall "release build"   # plus the notes matching these words
am remember --kind fix --title "…" --body "…" --evidence "…"   # save an approved note
am share                    # commit only memory-bank/ changes and push them
am refresh                  # bring teammates' memory into this working tree
am doctor                   # every check, in plain words
```

The agent does the remembering: at a milestone (a confirmed fix, a decision, a
finished task, your correction) it suggests one short memory and asks you; only
what you approve is saved. `am remember` also reads JSON on stdin; see
[skills/agent-memory/templates/remember.example.json](skills/agent-memory/templates/remember.example.json).

`am share` never touches your code. It commits the bank on top of the shared
branch (`git config agent-memory.authoritybranch`, default the remote's default
branch) and pushes. When that branch is protected it pushes a memory branch and
opens a pull request with `gh`. `am refresh` merges others' memory into your
working tree, keeping your own edits; overlapping edits get ordinary conflict
markers to resolve.

Afterwards a plain `git pull` works, whether your git merges or rebases on pull.
When your branch tracks the shared branch and only memory commits lie between
them, `am` moves the branch to the shared memory (nothing is staged). When
your git rebases on pull, or your branch has its own commits, `am` adds one
commit that changes only `memory-bank/`; a rebasing pull drops it once the
shared branch holds that memory. Otherwise (a merging pull that
fast-forwards) the shared memory is staged. `am` says which it did, and names
the step first needed where a plain `git pull` cannot work: `am refresh` again
when more memory was shared before you pull; with a rebasing pull, committing
or stashing uncommitted changes (`am share` for memory edits); with
`pull.ff=only`, `git pull --rebase` or `git pull --no-ff` for its memory
commit; and it says when the branch has no upstream to pull from.

## The memory bank

- `memory-bank/MEMORY.md` — the route map: each file, what it is for, when to read it.
- `projectbrief.md`, `productContext.md`, `activeContext.md`, `systemPatterns.md`,
  `techContext.md`, `progress.md` — the six context files.
- `topics/<id>.md` — one lesson per file, with record frontmatter
  ([schema](skills/agent-memory/schemas/record.json)), linked from `MEMORY.md`.

Use another directory with `git config agent-memory.bankroot PATH`.

## Development

```sh
cargo build --release
cargo fmt --check
scripts/qa.sh "$PWD"          # real-use QA in a temporary HOME, with timings
```

Linux release binaries cross-build with the bundled linker:

```sh
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld cargo build --release --target x86_64-unknown-linux-musl
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld cargo build --release --target aarch64-unknown-linux-musl
```

`am install --development-source PATH` installs a checkout that already holds
a built `target/release/am`. Releases are packaged and signed with
`cargo run --release --features dev-tools --bin am-dev -- release build|publish …`.
