# Portable runtime installation

`am install` is the installer compiled into the static `am` binary
(`src/installation.rs`). `install.sh` is the POSIX `sh` bootstrap for that same
`install` operation. This contract covers #46; update, uninstall, diagnostics and
the complete installed command surface retain separate execution tasks.

The AWA-style fresh-machine entry point (#153) requires no existing checkout:

```sh
curl -fsSL https://raw.githubusercontent.com/robertelee78/agent-memory/main/install.sh | sh
```

The bootstrap selects the release target from `uname -s`/`uname -m`. Only
`aarch64-apple-darwin`, `x86_64-unknown-linux-musl` and
`aarch64-unknown-linux-musl` have a signed `am`; other hosts refuse with
`unsupported-target`. It requires `curl` and OpenSSH `ssh-keygen`, creates a
private 0700 temporary directory, downloads the per-target `am` binary and its
`.sshsig`, and verifies them with `ssh-keygen -Y verify` against the allowed
signer pinned in the script (namespace `agent-memory.release-binary`). Only then
does it run `am install --bootstrap-binary PATH` with the forwarded options. The
binary refuses unless the signed release manifest's version and target digest
match its own bytes, then stages the signed portable pack and installs those same
bytes; it never downloads the binary again. The temporary directory is removed on
every exit path, including handled signals. Failures before the binary runs print
one fixed `agent-memory/bootstrap-failure-v1` JSON line. `--help` requires no
download, and option syntax is checked before any download. Running `./install.sh`
from a checkout performs the same signed download; it does not install that
checkout.

```sh
./install.sh --check
./install.sh
```

Signed releases are the only installation channel. The owned runtime lives
under `${XDG_DATA_HOME:-$HOME/.local/share}/agent-memory/runtime`. Managed Git
`--remote`/`--ref` channels are refused with re-bootstrap guidance; an existing
Python-era managed checkout is adopted read-only and left in place. An unexpected
source, unrelated runtime destination or changed signed artifact refuses. An
explicitly selected local source is never copied into or mutated as a managed
runtime:

```sh
target/release/am install --development-source "$PWD" --presets none --target-dir /absolute/custom/skills --mode copy
```

A development source must already contain a prebuilt `target/release/am`; the
installer never invokes Cargo and refuses with that path when it is missing. A
source that is not itself a checkout root is identified as `unversioned`; a
checkout reports its actual revision and dirty qualification. Git and native
atomic no-replace/exchange on Linux or macOS are required; no interpreter or
Cargo is needed for a signed installation. No agent client, credentials,
principal registry or existing bank is needed for installation.

The default `all` preset creates generic, Codex, Claude and OpenCode discovery
paths, plus small bounded activation shims in the relevant client instruction
files. `--presets` accepts `none` or a comma-separated selection of `agents`,
`codex`, `claude`, `opencode`. Repeated `--target-dir` adds custom discovery
roots. `--no-guidance` omits shims. The commands default to `~/.local/bin/am`,
with an owned `agent-memory` compatibility command in the same directory; both
are receipt-owned links to the runtime's `bin/am`. `--bin-dir` overrides the
directory. A source without the additive `installed_commands` declaration
retains only its legacy command so old planners are not stranded. Every path
argument is absolute. Link and copy modes bind the same complete artifact digest.

The public JSON plan reports every target's before/after identity, source
revision/version/dirty qualification, adapters and repository-local memory policy.
Check/dry-run leaves targets and installation records unchanged.

Activation follows complete target inventory/preflight, staging, pre-activation
backup snapshots, native atomic replacement of each target, target/package
readback, then atomic receipt publication. A nonempty managed copy uses native
exchange; it is never renamed away leaving an absent target. Filesystems are
probed before target activation. The system makes no claim of global atomicity
across targets. Existing wrappers, links, copies, files and sentinel blocks need
matching ownership. The installer never adopts a coincidentally matching
unrelated target. Edits outside an owned sentinel block remain intact; edits
inside it refuse. Stale/duplicate/missing blocks remain a reconciliation task.

Codex and Claude share hook JSON with other tools. Lifecycle ownership verifies
the complete receipt-owned prompt group and each recorded Stop group exactly
once, rather than treating the historical whole-file hash as ownership of other
settings. Malformed or duplicate-key JSON, changed or duplicate owned groups,
unsafe paths, mode changes and concurrent edits still refuse. Insufficient legacy
entry evidence remains unverified; a lookalike is never adopted. Transaction
backups, activation readback and uninstall-output recovery retain exact whole-file
identities. OpenCode's plugin link, wrappers and client artifacts retain strict
identity checks.

The authoritative receipt is
`${XDG_STATE_HOME:-$HOME/.local/state}/agent-memory/install/receipt.json`.
It binds the installation ID, source, artifact, enabled targets, pre/post target
identities, backup identities/paths, created directories, runtime state and data
ownership. The journal, historical journals and receipt backups live beside it,
outside the owned runtime. Replaced nodes and their pre-activation snapshots are
retained beside their respective target, with their exact paths in the receipt.
An unchanged reinstall preserves the receipt, installed targets and loaded
content and returns no restart guidance. Copy/link conversion with identical
content also needs no restart; newly enabled or changed loaded content does.

Ordinary activation errors roll back. A killed process leaves a journal. Run
`am recover`. If the installed command is unavailable, download the signed
`am-<target>` binary and its `.sshsig` for this host from the release, verify
them with the `ssh-keygen -Y verify` command and allowed signer pinned in
`install.sh`, and run that verified binary directly:

```sh
./am-aarch64-apple-darwin recover
```

Recovery verifies a receipt that reached its commit point or restores an
incomplete target set using recorded identities. It preserves post-interruption
user edits and reports a conflict instead of overwriting them. The process lock
uses OS advisory locking; a dead owner does not leave a permanent lock. Journals
and backups survive code replacement. A lost success response after receipt
activation is recovered as committed, and repeating install converges.

Project Memory Banks are not installer-owned data roots. Installation does not
initialize, migrate, commit, publish or remove them. The [portable guide](../../skills/agent-memory/lifecycle/install/GUIDE.md)
provides the progressive agent workflow. Installer public-command tests
(`tests/t2_installation.rs`, `tests/t2_bootstrap.rs`) run only in disposable
homes and client/config roots; the bootstrap tests use a throwaway signing key
and `file://` release assets.

Native API references: [Apple XNU rename flags](https://raw.githubusercontent.com/apple-oss-distributions/xnu/refs/heads/main/bsd/sys/stdio.h)
and [Linux renameat2 semantics](https://man7.org/linux/man-pages/man2/renameat2.2.html).
