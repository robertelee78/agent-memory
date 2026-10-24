# Portable runtime installation

`scripts/installer.py` invokes the installer carried inside the portable package.
`install.sh` is a local-checkout and standalone/stdin bootstrap for that same `install` operation. This
contract covers #46; update, uninstall, diagnostics and the complete installed
command surface retain separate execution tasks.

The AWA-style fresh-machine entry point (#153) requires no existing checkout:

```sh
curl -fsSL https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh | bash
```

The Bash 3.2+ bootstrap checks Python 3.11+ and Git, stages a temporary checkout
of the selected managed remote/branch, and forwards installation arguments to
the existing Python installer. The temporary loader is removed on normal exit,
failure and handled interruption; the owned runtime never points into it.
`--help` requires no source download. Explicit `--remote`/`--ref` options select
the same channel for bootstrap and installation. A complete local checkout is
resolved from the script's location, independently of the caller's directory.

```sh
./install.sh --check
./install.sh
```

Normal installation stages a clean Git clone of the recorded remote/branch under
`${XDG_DATA_HOME:-$HOME/.local/share}/agent-memory/source`. The initial default
channel is HTTPS `main`; it is not a signed immutable release channel. Reinstall
uses the recorded revision, not an implicit fetch/update. An unexpected channel,
branch, source revision, dirty managed checkout or unrelated destination refuses.
An explicitly selected local source is never copied into or mutated as a managed
checkout:

```sh
./install.sh --development-source /absolute/path/to/checkout --presets none --target-dir /absolute/custom/skills --mode copy
```

A complete copied package also works as `--development-source`. It is identified
as `unversioned` when it is not itself a checkout root. A checkout reports its
actual revision and dirty qualification. Python 3.11+, Git, and native atomic
no-replace/exchange on Linux or macOS are required. No agent client, credentials,
principal registry or existing bank is needed for installation.

The default `all` preset creates generic, Codex, Claude and OpenCode discovery
paths, plus small bounded activation shims in the relevant client instruction
files. `--presets` accepts `none` or a comma-separated selection of `agents`,
`codex`, `claude`, `opencode`. Repeated `--target-dir` adds custom discovery
roots. `--no-guidance` omits shims. The command wrapper defaults to
`~/.local/bin/agent-memory`; `--bin-dir` overrides its directory. Every path
argument is absolute. Link and copy modes bind the same complete artifact digest.

The public JSON plan reports every target's before/after identity, source
revision/version/dirty qualification, adapters and repository-local memory policy.
Check/dry-run leaves targets and installation records unchanged. A fresh managed
check uses an ephemeral clone that is removed after inspection.

Activation follows complete target inventory/preflight, staging, pre-activation
backup snapshots, native atomic replacement of each target, target/package
readback, then atomic receipt publication. A nonempty managed copy uses native
exchange; it is never renamed away leaving an absent target. Filesystems are
probed before target activation. The system makes no claim of global atomicity
across targets. Existing wrappers, links, copies, files and sentinel blocks need
matching ownership. The installer never adopts a coincidentally matching
unrelated target. Edits outside an owned sentinel block remain intact; edits
inside it refuse. Stale/duplicate/missing blocks remain a reconciliation task.

The authoritative receipt is
`${XDG_STATE_HOME:-$HOME/.local/state}/agent-memory/install/receipt.json`.
It binds the installation ID, source, artifact, enabled targets, pre/post target
identities, backup identities/paths, created directories, runtime state and data
ownership. The journal, historical journals and receipt backups live beside it,
outside the checkout. Replaced nodes and their pre-activation snapshots are
retained beside their respective target, with their exact paths in the receipt.
An unchanged reinstall preserves the receipt, installed targets and loaded
content and returns no restart guidance. Copy/link conversion with identical
content also needs no restart; newly enabled or changed loaded content does.

Ordinary activation errors roll back. A killed process leaves a journal. The
trusted source helper remains usable before the wrapper exists:

```sh
python3 -I skills/agent-memory/scripts/installer.py recover
```

Recovery verifies a receipt that reached its commit point or restores an
incomplete target set using recorded identities. It preserves post-interruption
user edits and reports a conflict instead of overwriting them. The process lock
uses OS advisory locking; a dead owner does not leave a permanent lock. Journals
and backups survive code replacement. A lost success response after receipt
activation is recovered as committed, and repeating install converges.

Project Memory Banks are not installer-owned data roots. Installation does not
initialize, migrate, commit, publish or remove them. The [portable guide](../../skills/agent-memory/lifecycle/install/GUIDE.md)
provides the progressive agent workflow. [Installer public-command tests](../../tests/contracts/test_installer.py)
run only in disposable homes and client/config roots, with isolated credentials
and file-only Git channels.

Native API references: [Apple XNU rename flags](https://raw.githubusercontent.com/apple-oss-distributions/xnu/refs/heads/main/bsd/sys/stdio.h)
and [Linux renameat2 semantics](https://man7.org/linux/man-pages/man2/renameat2.2.html).
