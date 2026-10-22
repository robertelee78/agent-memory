# Memory lifecycle model

Generated from `schemas/lifecycle.json` (agent-memory/lifecycle-v1).
Changes MUST update the machine definition and regenerate this file.

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
