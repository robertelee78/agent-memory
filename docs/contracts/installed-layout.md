# Installed command and storage integration

The lifecycle components ship as one runtime. The installed `am` command and
compatibility `agent-memory` wrapper expose `install`, `uninstall`, `update --check`, `update`, `version`,
`doctor` and `status`. Install/update/uninstall use verified receipts;
`version`, `status`, `doctor` and lifecycle previews return machine-readable
observations with their qualifications. The trusted portable helpers remain
usable before installation and after removal or a broken wrapper. Governed
bank operations and their complete installed CLI roundtrip remain the separate
ADR-0004 integration story; these lifecycle checks do not claim that release gate.

| Responsibility | Location and ownership |
| --- | --- |
| Managed runtime | `${XDG_DATA_HOME:-$HOME/.local/share}/agent-memory/source`; a verified signed release, or an explicitly recorded custom Git channel, with `skills/agent-memory/` as its portable artifact. |
| Installed commands | `$HOME/.local/bin/am` and compatibility `$HOME/.local/bin/agent-memory` by default, or the explicit `--bin-dir`; both owned wrappers resolve to the same recorded portable runtime. |
| Lifecycle state | `${XDG_STATE_HOME:-$HOME/.local/state}/agent-memory/install/`; receipt, journal, stable lock, owner metadata and receipt history survive source removal. |
| Replacement backups | Exact paths recorded in the receipt, beside affected targets or in installer state. They stay outside the active managed source and survive uninstall. |
| Durable project memory | One repository/worktree-local bank, selected by the established locator and optional `agent-memory.bankRoot` setting. It is not installer-owned data. |
| Local identity metadata | `AGENT_MEMORY_CONFIG_HOME`, otherwise `${XDG_CONFIG_HOME:-$HOME/.config}/agent-memory`; developer/project selection is independent of runtime installation. |
| Governed write/audit evidence | `AGENT_MEMORY_STATE_HOME`, otherwise `${XDG_STATE_HOME:-$HOME/.local/state}/agent-memory`, with per-bank write/operation and private audit subdirectories separate from `install/`. |
| Disposable references and caches | Explicit caller-selected private files/directories, such as separate cache and reference-index directories under XDG cache. Native v1 has no global cache or generated-index service registry; lifecycle does not adopt these files. |

The default signed source has kind `release` and stores only the portable skill
plus verified release metadata/signature. It has no Git checkout or development
bank. An explicit custom managed Git installation keeps its recorded origin/ref;
a source clone may physically include development memory without granting it
consumer identity. Runtime removal never owns a consumer repository bank.

The durable bank includes its Markdown router, context, typed records, topic
indices, history and content-free deletion markers. Canonical topic-index
Markdown is project memory, not a disposable generated index. Optional JSON
reference projections contain canonical ID/version/path references and source
hashes rather than duplicate lesson bodies. Losing those derived files does not
remove the canonical learning or require bank reinitialization.

The default `all` presets resolve to these discovery roots:

| Preset | Discovery root |
| --- | --- |
| `agents` | `$HOME/.agents/skills` |
| `codex` | `${CODEX_HOME:-$HOME/.codex}/skills` |
| `claude` | `${CLAUDE_CONFIG_DIR:-$HOME/.claude}/skills` |
| `opencode` | `${XDG_CONFIG_HOME:-$HOME/.config}/opencode/skills` |

Each root receives `agent-memory/`; relevant client instruction files receive
owned sentinel blocks unless guidance is disabled. `--target-dir` can be repeated
for custom harnesses. `--presets none` with custom roots creates only those
discovery targets. With no custom roots it creates no client links or activation
blocks while retaining a functional command wrapper and managed source. Neither
mode requires a client executable, developer bootstrap or initialized bank.

New artifacts declare `installed_commands: ["am", "agent-memory"]` in runtime
compatibility metadata. Absent declarations retain the legacy `agent-memory`
name only. Reinstalling through newer source helpers does not add aliases to an
older selected runtime whose planner cannot preserve them. The actual pre-alias
updater upgrades source using its already-loaded old planner; existing users
therefore run `agent-memory update` followed once by `agent-memory install` to
activate `am`. Subsequent `am update` operations plan and verify both wrappers.
Receipts remain the authority for which command paths are actually owned;
declarations do not imply that an unowned path exists.

Link mode resolves every client to the same portable artifact. Copy mode keeps
separate physical copies bound to the same complete byte/mode digest; guarded
update switches and verifies every owned copy. Installation, update and ordinary
uninstall retain canonical memory, project Git/index state, local attribution
and audit evidence, derived files and unrelated client settings/skills. Removal
decommissions the runtime receipt but retains it and its backup evidence outside
the deleted source; a trusted outside helper can reinstall against the same bank.
Data purge remains a distinct exact-preview and explicitly confirmed operation.

`tests/contracts/test_installed_layout.py` exercises actual public commands in
temporary homes and file-only Git channels. Its link/copy scenarios verify every
default client plus a custom client, two custom-only targets and no-client mode;
they inspect physical targets and per-client artifact digests. The all-client
scenario runs the installed lifecycle, a real fast-forward update, removal and
reinstall while preserving an existing bank, a real generated reference
projection, private identity/audit state and staged unrelated source. Its opaque
cache fixture verifies non-ownership, not an optional cache backend. It also
removes known disposable references/cache files and reads native Markdown through
the outside helper. These integrated checks supplement component suites; the
complete portable release matrix and every-write fault/concurrency gates remain
separate release requirements.
