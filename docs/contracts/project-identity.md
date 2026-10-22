# Stable project identity and repository evidence

One bank has one random UUIDv4 in `memory-bank/MEMORY.md`. The same bank ID
travels with committed Markdown across worktrees, clones and machines. Directory
names, a harness name and active credentials MUST NOT select another bank.
Developer IDs supply separate provenance; this registry stores repository
identity metadata, never personal memory. Repository and OS permissions remain
the security boundary; a project ID is not an ACL or authentication.

The portable algorithm lives in `agent_memory/project_identity.py` and
`remote_identity.py`. Clients MUST call it rather than infer an ID themselves.
From a checkout, its public entry point is:

```sh
python3 -I scripts/project_identity.py status --repo /path/to/repository
python3 -I scripts/project_identity.py preview --repo /path/to/repository \
  --action create --reason 'Initialize this new repository bank' > proposal.json
python3 -I scripts/project_identity.py apply --proposal proposal.json
```

A copied pack uses `scripts/project_identity.py` inside that pack. These helpers
require Python and Git, and have no installed CLI, attribution registry, existing
bank, hosting login, network or credentials prerequisite. Status and preview
write nothing, including on first use. Every operation emits one JSON document.
Exit 0 means the operation completed, exit 1 means refusal and exit 2 means
invalid CLI arguments. `status` can complete while `resolved` is false; callers
MUST inspect that field and its issues before a mediated memory write.

## Shared bank evidence

The required router fields remain schema, project ID and router time. A known,
optional `canonical-remote` field carries sanitized authority evidence:

```yaml
canonical-remote: "repo://github.com/owner/repository"
```

This is a repository identifier, **never a fetch or push URL**. Actual network
operations MUST use the configured Git remote and ordinary repository rules.
A project without a remote omits the field. The common router reader/renderer
validates this field; clients MUST NOT implement a second frontmatter parser.
Old minimal banks remain readable without it. A remote-backed bank without an
anchor needs an explicit binding decision before guarded writes; its omission
MUST NOT silently establish a new authority.

The anchor lets a new machine notice that a fork copied the original project's
UUID and original authority, even when that machine has no registry history.
A mismatch MUST stop resolution. Restoring an old bank UUID or authority later
MUST likewise fail against an incompatible checkout binding.

## Evidence and remote choice

Discovery resolves the real repository root and common Git directory. Linked
worktrees share the common directory's filesystem identity, so they join one
checkout entry; independent clones have separate entries. A non-Git directory
uses its own filesystem identity. A directory move on the same filesystem keeps
this key and its bank ID. Copying across filesystems, restoring a repository or
creating an independent checkout requires explicit link/fork clarification;
missing metadata MUST NOT generate a replacement bank ID automatically.

`agent-memory.authorityRemote` in Git configuration selects the authority.
An explicit `--authority NAME` on a binding preview can select it for that
checkout without changing Git configuration. The decision is retained locally;
a later configured authority takes precedence. Without either, `origin` is
usable only when all plausible remotes identify the same repository. A single
remote with another name still needs an explicit choice. Conflicting fetch and
push URLs, multiple plausible authorities, missing selected remotes and later
unexpected changes MUST stop resolution. A saved proposal MUST refuse if the
bank bytes, repository location/evidence, configured authority or registry
changes before apply. No Git configuration is changed by these operations.

Normalization removes URL credentials, queries, fragments, trailing slashes and
a terminal `.git`, lowercases the host, and joins HTTPS, SSH and SCP-style
host/path syntax. Default protocol ports disappear; other ports remain part of
the identity. GitHub.com owner/repository paths are case insensitive according
to its documented rule; unknown hosts preserve path case. Different hosts,
owners, repositories, forks and nondefault ports remain distinct. SSH host
aliases, custom remote helpers and server-side redirects are not guessed into
equivalence; explicit supported binding or configuration repair is required.
Local path/file remotes become canonical absolute file identifiers and cannot
promise portable remote identity on another computer. Normalization output and
errors MUST NOT expose URL credentials or query values.

The syntax and case decisions follow primary documentation:
[Git URL syntax](https://git-scm.com/docs/git-clone#_git_urls),
[Git common directory](https://git-scm.com/docs/git-rev-parse#Documentation/git-rev-parse.txt---git-common-dir),
and [GitHub repository parameters](https://docs.github.com/en/rest/repos/repos#get-a-repository).

## Explicit decisions and history

The local file is `${AGENT_MEMORY_CONFIG_HOME}/projects.json`, defaulting to
`${XDG_CONFIG_HOME:-$HOME/.config}/agent-memory/projects.json`. It binds project
IDs, sanitized authority identities, checkout evidence, observed locations,
operation receipts and append-only decision history. `schemas/projects.json`
names the supported fields. Unknown schemas/fields and unrelated or symlinked
registry files MUST be preserved and refused.

| Action | Decision |
| --- | --- |
| `create` | Reserve a new random ID for a location without a bank or checkout binding. |
| `bind` | Explicitly adopt a previously unregistered existing bank ID at its first local checkout. |
| `link` | Join an independent checkout or moved location to a known project with matching bank ID and authority. |
| `rebind` | Keep the bank ID and explicitly accept a changed authority; retain old remote aliases and the decision reason. |
| `fork` | Reserve a fresh project ID for an existing bank/checkout; retain prior project attribution in checkout history. |

The same canonical authority MUST NOT silently acquire two bank IDs. A bank
already registered at an independent location MUST require `link` or `fork`,
not another automatic adoption. Historical remote aliases explain earlier
bindings; they MUST NOT silently authorize a changed current remote. Explicit
linking can repair an intentionally changed checkout binding, and MUST preserve
its prior binding in history. Selecting a different developer does not split
these projects or create a separate bank.

**This foundation applies only local registry metadata.** Its preview includes
`bank_identity`, the desired shared identity fields. Applying `create`, `fork`,
or `rebind` can leave bank initialization or identity changes pending; the
receipt's nested `binding_status.resolved` remains false until they agree.
`verified: true` proves the registry write/readback or known operation replay,
not a bank mutation, admission or publication. The dependent scope/bind and
initialization operations MUST apply bank changes through the common observed
write/transaction guards and verify both surfaces. They MUST NOT relabel an
existing bank by copying an entire rendered snapshot. This split permits first
setup without requiring a bank that can only be created after identity resolves.

## Concurrency and recovery

Apply takes an exclusive `.projects.lock`, records operation/host/PID/start,
checks observed bytes, writes a private same-directory temporary file, flushes
it, replaces the registry atomically, flushes the directory and reads it back.
A concurrent proposal with older observations MUST refuse rather than overwrite
the winner. Each operation ID binds one exact proposal. Replaying a successful
operation MUST verify its ledger entry without applying it again, including
after a later rebind/fork. A moved original location can remain unresolved in
the replay receipt while its completed registry operation stays known.

Status MUST remain read-only. A leftover lock blocks new writes. Inspect a
bounded recovery proposal, then apply that exact proposal:

```sh
python3 -I scripts/project_identity.py recover > recovery.json
python3 -I scripts/project_identity.py recover --proposal recovery.json
```

Automatic recovery requires a recognized same-host lock whose process is
provably gone, supported advisory file locking, and unchanged lock inode/bytes.
A live or foreign owner, another writer/recovery, unknown state, changed lock or
unsupported locking MUST stop for operator review. The helper holds the lock's
file descriptor through removal so competing recovery cannot remove a newly
created writer lock. Config root and registry paths MUST use canonical paths
without symlinks. These are cooperative process guards within the stated OS
security boundary; they do not isolate mutually distrusting processes sharing
one account.
