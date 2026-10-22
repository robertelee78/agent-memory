---
id: ADR-0011
status: accepted
approval-scope: v0.0.1
date: 2026-10-21
depends-on: [ADR-0001, ADR-0004, ADR-0010]
---

# ADR-0011: Stable developer attribution and project identity

Accepted for **v0.0.1 only**, as recorded in the
[operator approval](../design/v0.0.1-approval.md). Later-milestone proposals
remain under review. Implementation, acceptance and delivery require separate
evidence.

## Context

Scope isolation depends on identity. Directory basenames, current client names,
and whichever Git credential happens to be active are not stable identities.
Clones, worktrees, forks, remote URL syntax, remote changes, and projects without
remotes otherwise cause silent namespace splits or merges.

## Decision

### Developer attribution

Installation SHALL create or adopt a random stable developer ID for provenance.
A human-readable label, operating-system account, Git author, GitHub login, and
client identity MAY be attached as evidence or aliases, but none alone SHALL
silently replace the stable ID. The local attribution registry stores identity
metadata only, never personal memories or preferences.

Selecting or creating a different developer identity SHALL be explicit.
Developers with repository access share the project bank; their attribution
identities MUST NOT create separate memory banks or imply separate access rights.

In v1 the operating-system account and filesystem permissions are the security
boundary. Developer IDs provide provenance, not private-memory selection or
authentication between mutually distrusting users of the same OS account. The
CLI SHALL state this and MUST NOT describe registry selection as an ACL.
Repository and OS permissions govern access; no per-developer memory access
control is promised.

Shared project-memory provenance SHALL identify the developer separately from
the harness and session. The same developer using two to five machines SHALL be
able to explicitly adopt the same stable developer identity on each. A different
developer SHALL retain a distinct identity. Git authors and hosting logins MAY help resolve aliases,
but MUST NOT silently change identity when credentials change. Conflicting or
unknown attribution SHALL remain visible during clarification; it MUST NOT be
misrepresented as a verified identity or authentication claim.

### Project identity

Every bank SHALL carry a random stable project ID in `MEMORY.md`. The local
registry SHALL bind that ID to observed repository evidence and locations.

For Git repositories, identity resolution SHALL examine the common Git directory
so linked worktrees resolve to the same project. Remote normalization SHALL:

- remove credentials, query strings, fragments, trailing slashes, and a terminal
  `.git`;
- lowercase the host and apply the hosting service's documented path-case rules;
- treat equivalent HTTPS, SSH, and SCP-style syntax as the same host/path;
- preserve the distinction between different hosts, owners, repositories, and
  forks.

If an explicit authority remote is configured, it wins. Otherwise `origin` MAY
be used when unambiguous. Multiple plausible remotes, a remote/bank-ID conflict,
or an unexpected remote change MUST stop and request an explicit bind, rebind,
fork, or new-project decision.

A repository without a remote SHALL keep its random project ID and location
binding. Moving the directory SHALL NOT create a new identity. Copying or
cloning a bank whose ID is already registered at another independent repository
location MUST require the user to link it as another checkout or fork it to a
new ID. A remote rebind SHALL preserve an auditable alias history.

### Resolution and repair

Read-only discovery MAY use the current directory and `MEMORY.md` as a fast
path. Before any mediated write or federation export,
the system SHALL resolve the stable developer and project IDs and reject
ambiguity.

`agent-memory status` SHALL show the resolved IDs and evidence. A dedicated
scope/bind operation SHALL preview changes to the registry and bank identity,
preserve the old aliases, and read back the result. No adapter MAY invent its
own repository-ID algorithm.

## Consequences

- Directory moves and worktrees do not fragment project memory.
- Forks and unrelated same-named repositories do not silently share memory.
- Developer attribution is not accidentally coupled to a client or credential.
- Initial setup and unusual remote changes require explicit decisions.

## Alternatives considered

- **Directory basename:** rejected because collisions and moves are routine.
- **Canonical remote only:** rejected because projects can lack remotes, change
  hosts, or have multiple legitimate remotes.
- **Active GitHub login as developer:** rejected because credentials are not the
  same thing as the developer who originated shared project learning.
