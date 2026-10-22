# Project bank router and file contract

The default shared project bank is `memory-bank/` at the repository root.
It MUST be tracked with the repository; it MUST NOT be gitignored as a whole.
One repository has one logical bank across its worktrees and clones. Developer
IDs are provenance and MUST NOT create personal banks or private overlays.
Disposable pending observations, caches, locks, retry cursors and recovery
state MUST NOT become another durable bank or be routed as shared memory.

The contract's machine-readable limits, field names, context names and allowed
directories are in
[`bank-router.json`](../../skills/agent-memory/schemas/bank-router.json).
The same portable helper renders and validates these rules from a checkout or
from a copied artifact. Its current boundary is router/file validation;
record lifecycle validation, evidence-led initialization and mediated mutation
have separate first-release stories.

## Router

`MEMORY.md` MUST start at byte zero with `---`, followed by known top-level
frontmatter keys and a closing `---` line:

```yaml
---
schema: "agent-memory/v1"
project-id: "8e0b66aa-2be5-4fa3-b47f-1245474aa7dd"
router-updated: "2026-10-22T17:00:00Z"
---
```

The project ID MUST be the project's resolved, canonical random UUIDv4 and
MUST remain stable. The example ID above is fixture data and MUST NOT be copied
as a new project's identity. `router-updated` is timezone-qualified RFC3339
and describes the router itself; it MUST NOT be used as every target file's
revision or freshness clock. The initial supported schema is `agent-memory/v1`.
An unknown schema MUST fail validation and MUST NOT trigger implicit migration.

The restricted frontmatter reader accepts top-level known keys; strings,
booleans, null and JSON numbers; JSON-compatible flow lists/maps; and indented
block lists of scalar strings. Canonical writers quote strings with JSON
syntax. Alias references, tags, duplicate keys (including flow-object keys),
unsupported nesting, malformed syntax and unknown fields MUST be rejected.
The router's three fields specifically MUST be strings matching their schema;
the broader grammar exists for subsequent typed records. Markdown bodies of
context and record files remain unrestricted.

The router MUST contain exactly one `## Routes` section with this table header
and five columns, and exactly one `## Omitted contexts` section:

```markdown
## Routes

| Path | Purpose | Scope | Lifecycle | Read when |
| --- | --- | --- | --- | --- |
| [projectbrief.md](<projectbrief.md>) | Goals and boundaries | repository | active | When resuming work |

## Omitted contexts

- `productContext.md`: No additional product evidence supplied
- `activeContext.md`: No current session focus supplied
- `systemPatterns.md`: No established patterns supplied
- `techContext.md`: No setup evidence supplied
- `progress.md`: No validated progress supplied
```

Every route MUST carry a relative Markdown path, concise purpose, applicability
scope, lifecycle state and read condition. Cells MUST be nonempty single-line
text without pipes or backslashes. Scope is `repository` or an explicit
`component:...`, `branch:...`, `worktree:...`, `task:...` or `session:...`
condition. Component paths MUST remain inside the repository. Conditions and
source links in target files SHOULD clarify applicability further; landing on
main MUST NOT make conditional guidance universal. Route lifecycle names are
candidate, quarantined, active, superseded, revoked, expired and deleted;
a route's existence MUST NOT imply its content is current guidance.

The router MUST stay within 16 KiB and 64 direct routes. Larger bodies belong
in project context; bounded topic-index traversal is owned by the disclosure
and optional-record stories. This initial validator accepts directly routed
optional Markdown files. It MUST reject traversal, absolute paths, duplicate
routes, symlinks, runtime/hidden targets, missing/empty files and orphan Markdown.
Ordinary path spaces use angle-bracket link destinations. Paths MUST NOT contain
ambiguous Markdown metacharacters, URL escapes, fragments or backslashes.

## Useful context

The six recognizable context files are:

| File | Purpose |
| --- | --- |
| `projectbrief.md` | Stable goals and boundaries |
| `productContext.md` | User problem and intended experience |
| `activeContext.md` | Current focus, changes, questions and next step |
| `systemPatterns.md` | Architecture and established patterns |
| `techContext.md` | Stack, setup, commands and constraints |
| `progress.md` | Validated capabilities, remaining work and known problems |

Initialization MUST create only useful supplied content, omit irrelevant files
and state why each missing context is omitted. An omitted context MUST NOT also
exist on disk. Six empty placeholder files MUST NOT be created to satisfy the
shape. Context files MAY use concise prose without metadata on each paragraph.
Volatile or authoritative claims MUST link to stronger code/tests, approved
ADRs, user decisions or AWA evidence when that is needed to establish their
authority. Memory MUST distinguish approved intent, observed behavior and
planned work; unsupported facts MUST NOT be invented.

Additional project Markdown MAY live under `topics/`, `episodes/`,
`procedures/` and `history/`; it MUST be reachable from the router. Runtime
transaction markers and locks MUST NOT be routes or enter shared commits.
A validator seeing an active transaction MUST report interruption and route to
recovery. A visible lock MUST stop validation until the writer finishes rather
than claim a stable current snapshot.

## Public file workflow

From a checkout, these commands were exercised by the isolated router suite:

```sh
python3 -I scripts/bank_contract.py render --input proposal.json
python3 -I scripts/bank_contract.py validate --repo /path/to/repository
```

The portable equivalent is `skills/agent-memory/scripts/bank_contract.py`.
Both commands emit one JSON document and write no bank or runtime files.
`render` emits a map of relative file names to Markdown contents; it MUST NOT
apply that proposal or overwrite existing memory. `validate` inspects only the
default bank and reports the `router-and-file-contract` scope. Exit 0 means a
rendered proposal or a valid file contract; exit 1 means refusal; exit 2 means
invalid command arguments. A successful render is not an admission decision,
verified mutation, or evidence that a bank has landed on main.

The [example input](../../skills/agent-memory/templates/router-proposal.example.json)
deliberately uses an invalid placeholder ID. Before rendering, the agent MUST
resolve the actual project ID, inventory existing memory, inspect applicable
sources and replace the example values. For an existing bank, it MUST preserve
useful content and reconcile the proposal rather than replace the bank from a
template. Unknown product decisions MUST trigger focused clarification.

A manual file workflow MAY create the reviewed proposal in a new bank and
commit its Markdown through ordinary repository rules. It MUST inspect and
reread the affected files, avoid visible locks/interrupted banks and MUST NOT
claim mediated concurrency or admission guarantees. Automatic initialization
and capture MUST use the common observed-write and identity guards when those
operations ship. Removing generated state MUST leave the router and routed
files sufficient for a fresh agent to resume from ordinary Markdown and links.
