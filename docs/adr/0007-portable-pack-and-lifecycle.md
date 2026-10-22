---
id: ADR-0007
status: proposed
date: 2026-10-21
related: [ADR-0003, ADR-0004]
---

# ADR-0007: Portable skill pack and installation lifecycle

## Context

The memory layer must work across Codex, Claude Code, OpenCode, and compatible
Agent Skills clients. Installation and update failures were a material weakness
of the system this project replaces. A memory system is especially sensitive to
brittle lifecycle behavior because a broken update can hide or corrupt the
state users rely on to recover work.

`agent-work-accountability` provides a useful distribution model: one complete
portable skill artifact, one managed source checkout, client discovery paths as
adapters, an idempotent installer, guarded fast-forward updates, version and
digest diagnostics, and end-user tests across target directories.

## Decision

The repository SHALL ship a complete portable artifact under
`skills/agent-memory/`. That directory SHALL contain the registered root
`SKILL.md`, progressive `GUIDE.md` tree, scripts, schemas, templates, and any
optional client-neutral assets required to operate the memory protocol.

The transaction sequence, ownership receipt, client targets, recovery rules,
and verification matrix are specified in the
[lifecycle blueprint](../design/lifecycle-blueprint.md).

Client-specific metadata and activation shims MAY live outside or in clearly
marked adapter directories, but the portable core MUST NOT depend on a client
API, hook system, marketplace, or orchestrator.

### Commands

The installed command SHALL provide at least:

```text
agent-memory install
agent-memory uninstall
agent-memory update --check
agent-memory update
agent-memory version
agent-memory doctor
agent-memory status
```

`install.sh` MAY remain the bootstrap entry point, but normal upgrades and
removal SHALL be available through the installed CLI.

### Managed installation

The default installation SHALL maintain one managed code checkout under an
XDG-compatible data location and link the same portable skill into each enabled
client's discovery directory. All clients MUST resolve to the same artifact so
their instructions cannot drift.

Runtime code, durable memory data, caches, and generated indices SHALL occupy
different paths. Updating or uninstalling runtime code MUST NOT delete durable
memory data.

The authoritative installation receipt SHALL live outside the managed checkout
at `${XDG_STATE_HOME:-$HOME/.local/state}/agent-memory/install/receipt.json`, with
recoverable receipt backups beside it. Removing or replacing the managed source
MUST NOT remove the evidence required to uninstall or roll back owned targets.

The default client targets SHOULD include:

- `$HOME/.agents/skills`;
- `${CODEX_HOME:-$HOME/.codex}/skills`;
- `${CLAUDE_CONFIG_DIR:-$HOME/.claude}/skills`;
- `${XDG_CONFIG_HOME:-$HOME/.config}/opencode/skills`.

Custom target directories and a no-client-links mode SHALL be supported.

### Install behavior

Installation SHALL be idempotent and non-destructive. It MUST:

- verify prerequisites before changing targets;
- identify every path it owns in a machine-readable installation manifest;
- refuse to replace an unrelated file, directory, or symlink;
- support symlink and copy modes;
- preserve a replaced managed copy as a recoverable backup;
- install command wrappers atomically;
- print the source revision, version, dirty qualification, artifact digest,
  enabled adapters, and data location;
- preserve user content outside sentinel-delimited activation blocks;
- tell running clients to restart only when their loaded artifact changed.

### Update behavior

Update SHALL lock against concurrent lifecycle operations. The managed channel
MUST be bound to its recorded remote and branch, MUST refuse a dirty checkout,
unexpected remote, unexpected branch, downgrade, or non-fast-forward history,
and MUST refresh each owned link or managed copy with an atomic per-target
replacement. Because targets may span filesystems, the system SHALL NOT claim a
single global atomic switch; a transaction journal, readback, and rollback SHALL
make partial activation detectable and recoverable.

`update --check` SHALL be read-only. A successful update SHALL verify the new
portable artifact before switching clients to it. If verification fails, the
previous installation SHALL remain active.

Runtime update SHALL NOT migrate project Memory Banks. Every release SHALL
declare its directly readable schema range and the older source schemas for
which it bundles a complete migrator chain. Update SHALL refuse when a discovered
bank is outside both ranges. Writing a new schema requires a separately
previewed, backed-up, explicit `migrate` operation so runtime rollback does not
silently strand data. Encountering an undiscovered older bank later SHALL refuse
ordinary mutation and offer the bundled explicit migrator when supported.

The general downgrade refusal does not prohibit automatic rollback to the exact
pre-transaction runtime recorded in the lifecycle journal. After an explicit
bank migration, rollback SHALL either prove the old runtime can read the new
schema, run a verified reverse migration, or restore the migration backup.

The initial HTTPS fast-forward channel MAY ship before signed releases, but the
trust limitation SHALL be documented. A later release channel SHOULD use
signed immutable bundles and a pinned verification key.

### Uninstall behavior

Uninstall SHALL remove only paths proven to be owned by the installation
manifest. It MUST preserve:

- durable memory events and projections;
- unrelated skills and client configuration;
- user content outside managed activation blocks;
- backups created during installation.

Removing durable memory SHALL require a distinct `--purge-data` operation with
an exact preview and explicit confirmation. Uninstall SHOULD leave a command
showing how to reinstall against the preserved data.

### Diagnostics and recovery

`version` SHALL report the CLI version, source revision, source path, artifact
digest, and whether the code checkout is clean.

`doctor` SHALL verify executable prerequisites, schema compatibility, lock and
directory permissions, client links, activation blocks, MCP configuration when
enabled, store readability, event-log integrity, projection freshness, index
rebuildability, and pending migrations. It SHALL separate warnings from failures
and print a bounded repair for each known failure.

`status` SHALL be cheap and read-only. It SHOULD report the resolved principal,
repository identity, active store, memory counts by lifecycle state, pending
capture, projection position, index freshness, and enabled client adapters.

Interrupted installs, updates, uninstalls, migrations, and projection rebuilds
SHALL be resumable or safely rollbackable. A lost response MUST be recoverable
by rerunning the same command.

### Verification

Tests SHALL exercise real user-facing commands in temporary homes and client
config roots. They SHALL cover fresh install, unchanged reinstall, guarded
replacement, copy mode, link mode, update check, update, interrupted update,
rollback, uninstall, preserved data, explicit purge, paths containing spaces,
multiple clients, and stale or user-modified activation blocks.

Pack validation SHALL run before publishing and before activating an update.
No test SHALL require real credentials or mutate the user's live client
configuration or memory store.

## Consequences

- Installation and data survival become explicit product contracts.
- One artifact serves every client while adapters remain replaceable.
- Lifecycle code and tests are substantial first-class work, not release
  packaging added at the end.
- Signed distribution remains a known follow-up unless implemented initially.

## Alternatives considered

- **Ask users to copy the skill manually:** rejected because copies drift and
  upgrades become unverifiable.
- **Use each client's marketplace as the authority:** rejected because the
  clients have different packaging and release behavior.
- **Delete data during ordinary uninstall:** rejected because runtime removal
  and user-data destruction are different decisions.
- **In-place updates without verification or rollback:** rejected because a
  memory layer must remain recoverable when distribution fails.
