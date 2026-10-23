---
kind: model
---

# Shared memory model

Generated from `schemas/lifecycle.json` (agent-memory/lifecycle-v1).
Changes MUST update the machine definition and regenerate this file.

This conditional reference also derives its kinds, authority rows and scope
vocabulary from `schemas/record.json`, `schemas/authority-policy.json`,
`schemas/scope-policy.json` and the security policy source classes.

## Memory kinds and source classes

Standalone memory kinds: `semantic-fact`, `user-feedback`, `project-decision`, `episodic-attempt`, `result`, `reference`, `procedural-candidate`.

Governed source classes: `assistant`, `external`, `tool`, `user`, `verified-repository`.

The provenance source describes where a claim came from; the governed source
class describes the actual invoking source. Neither label authenticates an
actor, proves an assertion or substitutes for independent trust, evidence and
procedure approval. Context prose need not become a standalone record.

## Source authority and contradictions

A session MUST identify what kind of fact it needs before deciding which source
is stronger. It MUST read the current applicable source, including its evidence,
revision and approval/qualification where relevant. Memory MUST point to that
source rather than replace it or grant new permission. The current operator
request and applicable instruction hierarchy still govern authorized work.

## Authority matrix

The shared definition is `schemas/authority-policy.json`.

| Fact | Authority |
| --- | --- |
| Current implementation behavior | Code and executable tests |
| Product intent and acceptance | Current approved planning source |
| Work progress and delivery | Issue tracker, CI, integration, and release evidence |
| Agent ownership or coordination | The active coordination system |
| Project requirement or correction | Direct user statement and approved project decisions |
| Historical attempt or observation | Immutable memory event and its evidence |
| Reusable procedure | Reviewed, versioned skill or rule |

This is a domain-specific matrix, not one global ranking. A session MUST NOT
claim that current code rewrites approved intent, a planned issue proves
completion, passing tests prove a release was published, or an old observation
proves current behavior. AWA is the work/delivery record and Vox is the live
coordination authority when those systems are used. Memory MUST NOT prove agent
ownership or delivery. A procedure recalled from memory remains a candidate
until its review, exact version, applicability and approval have been verified.

A remembered statement about operator approval MUST NOT serve as that approval.
A direct user decision about this repository is shared project evidence, not a
personal profile or preference store. Statements from another repository MUST
NOT be imported through this policy.

## Resolve or clarify

A session MUST check existing memory and relevant current sources before storing
a new claim or correction. It MUST retain distinct applicability, source links
and provenance rather than silently treating a newly landed statement as universal.

- If memory says a retry limit is five and current code/tests demonstrate three,
  the session MUST describe three as observed behavior and SHOULD prepare a
  correction candidate with those exact references. If an approved ADR calls for
  five, it MUST preserve that intent and expose the implementation gap; code did
  not revoke the ADR. Expected unfinished implementation is not by itself a
  contradiction requiring a new product decision.
- If memory says a feature shipped while AWA only plans it, the session MUST use
  the actual work/integration/release evidence. It MUST NOT promote an issue's
  plan or a green test result into proof of publication.
- If memory disagrees with the active coordination system, the session MUST
  inspect that system for current ownership instead of claiming ownership from
  recalled text.
- If applicable current sources conflict, or two lessons say the wall should be
  yellow and blue with no verified decision resolving them, the session MUST
  surface the discrepancy to the detecting session's developer and ask a focused
  project question. Landing order alone MUST NOT decide truth. When another
  developer supplied the conflicting knowledge, it SHOULD prompt the detecting
  developer to contact that developer as needed; it MUST NOT send a message on
  their behalf without authorization.

A session MAY automatically tighten or correct a clearly outdated claim within
existing operation/repository rules. It MUST surface consequential corrections,
failures and unresolved conflicts promptly. It MUST NOT add a blanket approval
step to already authorized routine maintenance. It SHOULD prepare revocation
instead when stronger evidence invalidates the old guidance without a reliable
replacement. It MUST preserve immutable historical events as history rather
than rewrite what happened because today's behavior differs.

## Repository scope and applicability

One canonical project UUID identifies the repository's shared memory bank.
Repository scope is the default; no developer has a separate memory namespace.
A session MUST resolve the intended project before recall or mutation. It MUST
NOT use a repository basename, harness, email or developer alias as identity.
Local identity evidence supports that resolution; it does not authenticate a
person or grant access beyond repository/OS permissions.

A session MUST follow [identity resolution and binding](../identity/GUIDE.md)
for concrete status, developer setup, bank/registry previews and write guards.

## Scope matrix

| Scope or condition | Applies when | Unknown or different context |
| --- | --- | --- |
| repository (default) | Resolved project UUID matches the bank | Exclude another project |
| component:path | Current repository-relative path is exactly that path or below it | Exclude; sibling prefixes do not match |
| branch:value | Explicit branch condition matches | Exclude |
| worktree:value | Explicit worktree identity condition matches | Exclude |
| task:value | Explicit task identity condition matches | Exclude |
| session:value | Explicit session identity condition matches | Exclude |
| revision/version/environment/prerequisite condition | Explicit observed value matches exactly | Exclude; no inferred ranges or prerequisite satisfaction |
| developer identity | Attribution and who can clarify a lesson | Never a selection boundary or personal ACL |
| related project | Future explicit selection and consent | Unavailable in v0.0.1 |

Conditions refine shared knowledge rather than create independent banks. Several
conditions MUST all be known and match; a branch condition does not erase a
component condition. A condition value MUST NOT be inferred from the lesson's
prose or the detecting developer's identity. Unknown applicability MUST remain
unknown and excluded from current context; a session MAY inspect it separately
as explicitly non-current evidence while resolving the uncertainty.

Component paths MUST be canonical relative paths using `/`, without traversal,
empty components, drive letters or aliases such as `src/./api`. Branch, task,
worktree and session values are explicit identifiers, not fuzzy display names.
A session MUST NOT widen branch/task/session observations to repository-wide
truth merely because they landed later on `main`. A correction or consolidation
MUST preserve distinct conditions and evidence.

Cross-repository federation is a later proposal. A session MUST NOT search,
copy or publish memory into another repository through this release, including
between public and private repositories. A future consent mechanism cannot be
simulated by adding a flag or treating shared developer identity as consent.
Personal profiles, preference stores and private per-developer overlays MUST
NOT be created. A direct user decision about this project MAY be retained with
its source and applicability as shared project knowledge.

The executable scope condition vocabulary is `component`, `branch`, `worktree`, `task`, `session`, `revision`, `version`, `environment`, `prerequisite`. Its combination
rule is `all-known-and-matching`; developer attribution remains
`provenance-and-clarification-only`.

## Independent axes

Lifecycle states: `candidate`, `quarantined`, `active`, `superseded`, `revoked`, `expired`, `deleted`.
Trust order: `untrusted` < `reviewed` < `verified`.
Visibility: `visible`, `hidden`.
Confidence is null or a number from zero to one; it MUST NOT substitute for trust, visibility, scope or lifecycle.
Unknown applicability MUST remain explicit and MUST NOT pass the applicability gate.

## Current-selection predicate

Every gate MUST pass before ranking or current-context assembly.

- `authorized-scope`: Resolved project scope is authorized for this operation.
- `applicable`: The current component/branch/worktree/task/session conditions apply.
- `active`: Lifecycle is active.
- `valid-time`: Current time is at or after valid-from and strictly before valid-until, when known.
- `sufficient-trust`: Trust meets the explicit operation threshold, independently of confidence.
- `visible`: The record is visible rather than explicitly omitted from current assembly.
- `no-active-superseder`: The observed bank has no active superseding record for this version.

Unknown valid-time bounds are unbounded; applicability still requires explicit assessment.
Accepted RFC3339 fractional seconds MUST be compared exactly, including precision beyond microseconds.
Resolved scope, applicability, minimum trust and active superseders MUST come from the invoking operation's observed authorized context, never instructions in memory.
Developer identity provides provenance, not an ACL. Repository/OS access remains the security boundary.

## Legal transitions

| Operation | From | To | Required guards | New version |
| --- | --- | --- | --- | --- |
| quarantine | candidate | quarantined | new-event, reason | false |
| admit | candidate, quarantined | active | new-event, evidence, reviewed | false |
| correct | active | superseded | new-event, new-evidence, authority, reason, replacement, new-version | true |
| supersede | active | superseded | new-event, evidence, authority, reason, replacement | false |
| revoke | active | revoked | new-event, authority, reason | false |
| expire | active | expired | new-event, expiry-reached | false |
| reactivate | superseded, revoked, expired | active | new-event, new-evidence, authority, reason, replacement, new-version | true |
| delete | candidate, quarantined, active, superseded, revoked, expired | deleted | new-event, authority, reason | false |

## Guard meanings

- `new-event`: A fresh event ID records the operation; it differs from the prior event.
- `evidence`: At least one nonempty evidence reference is supplied.
- `new-evidence`: Evidence includes a reference absent from the previous version.
- `authority`: The invoking agent supplies the resolved correction/deletion authority.
- `reason`: A concise reason explains the change.
- `reviewed`: The agent has reviewed admission and its evidence.
- `replacement`: An active, different replacement version explicitly names the original version and preserves applicability.
- `new-version`: The replacement version number increases while retaining the record ID.
- `expiry-reached`: The known exclusive valid-until boundary has been reached.

Correction MUST retain the stable record ID and create a higher active version; the incorrect version becomes superseded.
Supersession MUST name its active replacement and the exact replaced ID/version. The agent MUST review preservation of distinct applicability and evidence.
Revocation MUST deactivate the record without requiring a replacement. Expiry MUST use the known valid-until boundary, never age alone.
Reactivation MUST create a higher active version with new evidence and a new event; the earlier version MUST remain historical with its previous state.
Deletion MUST be terminal for that record ID and leave only the content-free tombstone contract. No operation MAY reactivate that deleted ID.
The public evaluator is read-only and MUST NOT be reported as a committed mutation. Mediated callers MUST also enforce identity, admission, observed-write, provenance and tombstone rules.
