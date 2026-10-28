---
id: ADR-0022
status: accepted
date: 2026-10-28
amends: [ADR-0012, ADR-0021]
---

# ADR-0022: Publish memory as an explicitly allowed push actor

## Context and root cause

The operator reported locally retained learning in repo-to-cve that could not be
published because main restricts push actors. Read-only GitHub inspection found
classic branch protection with one allowed user. The Mac API credential's stable
user ID matched that user; this does not establish another machine's actor.
The publication policy unconditionally marks any non-null classic `restrictions`
object unsupported. Thus an allowed actor cannot even prepare publication.
This is a missing supported policy case, not evidence that GitHub rejected a push.

The accompanying Codex Stop notice is separate: a source observation has no
verified no-learning or published completion and its bounded continuation cap
was reached. A local admission or denied publication must not be relabeled as
published, no-learning, or a completed source assessment. No change to those
truthfulness gates is authorized by this correction.

## Decision

### T1: Explicit allowed-user publication

For classic push restrictions, AM MUST read the authenticated GitHub user and
all pages of the branch's allowed-user endpoint. It MUST validate positive integer
user IDs and logins and match the stable ID together with case-insensitive login.
Only an explicit matching user permits the existing supported publication path.
Repository write permission or admin status alone MUST NOT establish eligibility.
Malformed, unavailable, incomplete or nonmatching observations MUST remain
pending with a specific cause and an actionable next step. API identity is not
proof of Git transport identity: GitHub still enforces the final ordinary push.
Team-only and App-only eligibility remain pending until supported with their own
complete evidence; this correction MUST NOT infer membership or install identity.
No credential switch, privileged override, force push, auto-approval or repository
protection mutation is permitted.

The validated actor and allowlist observations MUST participate in the policy
fingerprint and be reobserved by the existing immediate pre-push policy check.
An actor or restriction change invalidates the pending candidate's eligibility.
Existing PR, required review/check/App identity, merge-object, semantic proof,
source pins, original-index preservation and authority readback gates MUST remain.
The supported protected-branch workflow remains in use, including for a branch
whose only classic protection is actor restriction. No new direct-push shortcut
is introduced.

When publication cannot proceed, diagnostics MUST distinguish locally available
learning from unverified publication and retain exact pending work. They MUST NOT
claim completed source assessment simply because a local memory file exists.
The operation guide MUST explain explicit-user support and remaining team/App
limits. This task does not remove the bounded hook continuation or manufacture a
new assessment outcome to hide genuine pending work.

## Verification and delivery

Bounded local protocol tests MUST exercise actual prepare/land commands and real
Git remotes: allowed user succeeds only through required reviews/checks; an
unlisted admin cannot prepare; malformed/unavailable or paginated allowlist
observations remain pending; actor/allowlist changes before landing refuse writes;
local bank, feature code, HEAD and index remain intact. Existing publication
policy, pending and recovery suites remain regression coverage. No live consumer
repository is modified by qualification. Independent review precedes landing.

The correction is tracked in v0.0.10; published releases remain immutable. This
source authorizes implementation, not a claim of acceptance or publication.

## References

- [GitHub branch-protection REST API](https://docs.github.com/en/rest/branches/branch-protection)
- [Protected branches and push restrictions](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches)
- [ADR-0012](0012-git-shared-memory.md)
- [ADR-0021](0021-unobtrusive-memory-boundaries.md)
