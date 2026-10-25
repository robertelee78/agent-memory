---
id: ADR-0015
status: proposed
date: 2026-10-25
amends: [ADR-0007]
related: [ADR-0013, ADR-0014]
---

# ADR-0015: Signed release channel

Proposed technical refinement of existing signing task #49. The operator selected
signed releases and established lifecycle patterns, then authorized a dedicated
AM signing key. Independent technical approval is still required. Implementation,
acceptance and delivery remain separate evidence.

## Context

ADR-0007 anticipated signed immutable bundles after its initial Git channel.
C2E already demonstrates the required signed manifest and verified-asset pattern.
AM needs that boundary for its portable Python/Markdown artifact; transporting
complete Git history and inventing ancestry exceptions adds avoidable machinery.

## Decision

The signed default channel SHALL use one immutable GitHub Release, a pinned AM
Ed25519 key, a signed manifest and a portable archive. The archive MUST contain
only the complete tracked portable skill; it MUST exclude this repository's
memory bank, Git history, tests, repository documentation and private state.

The manifest SHALL bind repository, tag, semantic version, source commit, archive
name/size/SHA256 and complete portable identity. Verification MUST occur before
candidate execution. The first fetched bootstrap remains an HTTPS trust anchor.
Private signing material MUST remain outside Git and consumer artifacts.

Installed signed artifacts SHALL use a distinct `release` source kind. Their
ordering SHALL use semantic versions; update MUST refuse a downgrade or a
same-version replacement with different portable identity. Git source ancestry
is provenance, not ordering for this channel. Existing custom managed-Git and
development-source policies MUST remain unchanged.

The new updater SHALL automatically transition a proven owned official default
Git/main installation to the signed channel, after old ownership and new release
verification. It MUST require a newer release version or exact same-version
portable paths/bytes and Git executable intent, retain the predecessor and use
the existing journal. For this legacy comparison, actual modes MUST first match
the original ownership receipt; Git executable intent SHALL determine equivalence
to canonical signed modes without mutating old source. This exception MUST NOT
permit replacement of an already-signed version with a different artifact. It MUST
NOT silently transition custom channels or claim that an old unsigned updater
performed signature verification. Diagnostics SHALL report the actual activation
trust path, including recovery and rollback.

The existing per-target activation, backups, private-path ownership, compatibility
and explicit memory-migration boundaries SHALL remain in force. Installation and
update MUST NOT initialize, bind or recall development memory as consumer memory.

The public one-line install, `am update`, `am --version` and `am uninstall`
journey SHALL remain simple. One publisher command SHALL own packaging, signing,
publication and remote readback. Signed assets MUST be publicly available and
verified before the default bootstrap requires them. Qualification SHALL use
local isolated macOS/Linux journeys rather than qualification releases or
expensive recurring hosted CI gates.

Details and acceptance obligations are in the
[technical design](../design/v0.0.3-signed-distribution.md). This refines the
existing #49 outcome; it does not create a second lifecycle or duplicate task.

## Consequences

- Consumer installations acquire a small signed portable artifact.
- Release updates no longer need a managed Git checkout or ancestry exception.
- Legacy unsigned activation remains distinguishable from verified activation.
- Publisher authentication does not prove code correctness or eliminate initial
  bootstrap trust.
