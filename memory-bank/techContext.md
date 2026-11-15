# Tech context

## Stack

- Rust (edition 2024, toolchain pinned in `rust-toolchain.toml`), about 6,000
  lines in `src/`; dependencies `sha2`, `flate2`, `libc` only.
- One static binary `am` (alias `agent-memory`); `am-dev` (feature `dev-tools`)
  only packages and signs releases.
- The skill pack `skills/agent-memory/` (SKILL.md, record schema, bank
  templates, the omp extension `hooks/omp.ts`) ships with every install.
- Targets: `aarch64-apple-darwin`, `x86_64-unknown-linux-musl`,
  `aarch64-unknown-linux-musl`.

## Commands

`am status`, `am doctor`, `am init`, `am recall [QUERY]`, `am remember`,
`am share`, `am refresh`, `am install` / `update` / `uninstall`,
`am hook session-start --client claude|codex|omp`. `am <command> --help` shows usage.

## Build and check

```sh
cargo build --release                 # zero warnings expected
cargo fmt --check
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld cargo build --release --target x86_64-unknown-linux-musl
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld cargo build --release --target aarch64-unknown-linux-musl
mkdir -p target/release && cp "$CARGO_TARGET_DIR/release/am" target/release/am   # if CARGO_TARGET_DIR is set
scripts/qa.sh "$PWD"                  # the quality gate: real use of am, timed
```

There are no unit or contract tests. `scripts/qa.sh` drives the real `am` in a
temporary HOME (install, init, remember, recall, share/refresh between clones,
plain `git pull`, refusals that protect data, hooks, doctor, uninstall, v0.0.14
takeover) and records each command's time. CI (`.github/workflows/build.yml`)
runs fmt, a warning-free build and `qa.sh` on all three targets;
`verify-release.yml` installs a published release the way a user does.

## Git use

Each command observes the repository once: root (found without git), then one
`git config` call for remote URL, `user.email` and the bank path
(`agent-memory.bankroot`, default `memory-bank`). `share` and `refresh` add only
the fetch/merge/push calls they need (about 14 and 10 git calls with changes).
All git calls use literal pathspecs. Targets: every command under 200 ms on a
normal repo excluding network; the session-start hook under 100 ms.
