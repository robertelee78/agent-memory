---
kind: operation
domain: maintain
---

# Install the portable runtime

## Use when

A session MUST use this operation to preview or activate an installation, or to
recover an interrupted installation before the installed command is available.
The installer owns runtime targets; project Memory Banks remain outside its
ownership.

## Required context

A session MUST first load the [maintain domain](../../maintain/GUIDE.md).
It MAY open the [shared model](../../model/GUIDE.md) for unresolved memory
semantics. Installation does not require a project bank or developer identity.

## Inputs and preconditions

A session MUST use Python 3.11 or later, Git, and a supported local filesystem.
A complete package is required to run the Python helper directly. The standalone
`install.sh` bootstrap obtains a temporary verified portable release when invoked
through curl-to-bash; it requires Bash 3.2+, curl and OpenSSH SSH-signature support
in addition to Python and Git. The default managed channel is the latest stable
GitHub Release with the pinned AM key. An explicit `--development-source` selects
a local checkout or copied complete package; the installer MUST NOT mutate that
source. Explicit Git sources and legacy old-process activations remain unsigned:
a digest alone binds bytes and modes, not publisher authenticity. Signed release
verification authenticates publisher bytes before candidate execution; the fetched
bootstrap itself remains the initial HTTPS trust anchor. The portable archive
contains no repository development memory, Git history or consumer bank binding.

The executing verifier MUST already be trusted. In-package checks cannot
authenticate Python code that has already loaded from an attacker-writable
runtime. If that exposure is possible, the session MUST use a trusted complete
outside helper for inspection and recovery.

Lifecycle state MUST be current-user-owned and private (0700); sensitive records
MUST be ordinary, single-link, current-user-owned and not group/other writable.
Runtime paths MUST pass the Unix ownership and ancestor-replacement policy before
use. An established private ancestor can protect descendant payload permissions;
it cannot rescue its own entry through an earlier writable non-sticky parent.
Root-owned sticky temporary parents permit only trusted-owned direct entries.
New owned copies stage behind a private root. These checks do not audit all OS
ACLs or grant trust to a different project Memory Bank.

An unsafe existing layout MUST be preserved and refused. The session MUST offer
owner review or private per-user XDG/client paths, not silently chmod user paths,
ignore hardlinks, or add a trusted-group exception. Shared repository-bank
permissions remain a separate policy from runtime and receipt ownership.

## Observe

The session MUST inspect the source, target plan and any external receipt at
`${XDG_STATE_HOME:-$HOME/.local/state}/agent-memory/install/receipt.json`.
Defaults activate generic Agent Skills, Codex, Claude and OpenCode discovery;
`--presets none` disables those links, and repeated `--target-dir` options add
custom discovery directories. `--no-guidance` omits activation shims.

Native omp is explicitly excluded from `all`. The session MAY select a reviewed
native binding only with `--presets omp --omp-agent-dir ABSOLUTE_ROOT
--omp-peers-upgraded`, after quiescing same-bank private-state writers and
upgrading their peer readers. The session MUST inspect the preview's executable
observer authority, retention limits and exact owned skill/extension/activation
targets. It MUST NOT infer an omp root from HOME/XDG/profile settings, change
permissions/provider settings, or treat a disk receipt as actual native support.
The [source-bound native setup and qualification matrix](https://github.com/robertelee78/agent-memory/blob/main/docs/testing/omp-native-qualification.md)
defines that separate evidence gate. `--no-guidance` omits the optional Markdown
activation block, not the explicitly selected native extension.

Managed code lives under the XDG data root, while receipts, journals and receipt
backups live under the XDG state root. Project memory stays in its repository.
The session MUST preserve those separate ownership domains.
Installation and reinstall also discover retained private recovery/source state.
For a pending layout frontier, the actual candidate must support
`layout-frontier-v1` and retain the bound external state. Repeat
`--layout-quiescence /private/path/renewal.json` for each bank whose independently
reviewed operator attestation must bind a new measured runtime or invocation
route. Each file contains the exact attestation/independent-review envelope;
check mode is nonpublishing, and apply publishes only after complete validation
under the bank source barriers. Installing a capable binary alone does not
authorize legacy routes or erase unresolved source obligations.


## Plan

For installation without an existing checkout, the session MAY use the AWA-style
one-line bootstrap documented in the
[project README](https://github.com/robertelee78/agent-memory#install-and-first-use),
including its optional preview and custom arguments.

The bootstrap delegates the same installer and removes its temporary source on
exit. The runtime and external receipt MUST remain in their separate durable
owned locations. Bootstrap scratch MUST NOT become a development source.

With `SKILL_DIR` resolved to a trusted complete package, the session MUST preview
the intended source and target configuration before activation. For example:

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/installer.py" install --check --development-source /path/to/checkout --mode copy --presets none --target-dir /path/to/custom/skills
```

The session MUST compare each target's current and planned identity. Check mode
leaves installation targets and records unchanged; a fresh managed-source check
uses private signed staging (or a clone for an explicit Git channel) and removes
it after inspection. Verification prerequisites are exercised before lifecycle
lock/owner changes; temporary verification files are separate from owned runtime
and bank writes. Changing enabled targets
would strand ownership and MUST be handled through a separately authorized
uninstall/reinstall when that operation is available.

## Apply

After an authorized acceptable preview, the session MUST run the same install
configuration without `--check`:

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/installer.py" install --development-source /path/to/checkout --mode copy --presets none --target-dir /path/to/custom/skills
```

The installer stages all targets, checks atomic replacement on each target
filesystem, activates each target, reads it back and records the receipt.
Native atomic exchange supports replacing managed nonempty copy directories.
The session MUST NOT describe separate filesystems as one globally atomic switch.

## Read back

The session MUST require exit 0 and `verified: true`, then read `receipt_path`,
`receipt.source`, target identities, backups, enabled adapters and data roots.
Source identity reports revision or `unversioned`, dirty qualification, version,
and artifact digest. The installed `am` and compatibility `agent-memory` wrappers expose installation
and recovery; the [maintain domain](../../maintain/GUIDE.md) routes update,
uninstall and diagnostics through their own operations.

## Verify

The session MUST verify that each enabled client resolves the receipt's artifact
digest and that existing user content remains outside the bounded activation
block. Copy and link modes identify the same package. An unchanged reinstall
returns `changed: false` and no restart list. Running clients SHOULD restart
only when listed in `restart_clients` because their loaded content changed.

## Outcomes

A verified receipt proves the recorded installation readback. It MUST NOT be
treated as a memory checkpoint, schema migration, published learning, source
signature or completion of update/uninstall diagnostics. Missing optional agent
clients do not prevent installing their discovery paths.

## Failure handling

Exit 1 with `verified: false` means the session MUST preserve the refusal and
reported paths. Unrelated files, directories, links, wrappers and activation
blocks MUST NOT be replaced. User edits outside an owned activation block remain
preserved; edits inside it require reconciliation. Ordinary activation errors
roll back; a killed process leaves an external recovery journal. Backups remain
available rather than being silently removed.

## Diagnostic fallback

When installation is interrupted, the session MUST use the trusted source helper
even if the installed command does not exist:

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/installer.py" recover
```

Recovery verifies a committed receipt or rolls back an incomplete activation.
The session MUST inspect its verified result before retrying installation. It
MUST preserve a recovery conflict caused by external edits rather than overwrite
those edits. A lost response after receipt activation is recovered as committed.
