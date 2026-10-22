# Shared lifecycle and current-selection contract

[`lifecycle.json`](../../skills/agent-memory/schemas/lifecycle.json) is the
versioned authority for state names, legal transitions, guards and current
selection. The public evaluator, router validator and
[generated model reference](../../skills/agent-memory/model/GUIDE.md) use that
same definition. A caller MUST NOT maintain its own lifecycle or currentness
table. The model guide MUST be regenerated when the definition changes:

```sh
python3 -I scripts/lifecycle_model.py definition
python3 -I scripts/lifecycle_model.py guide
python3 -I scripts/lifecycle_model.py current --input selection.json
python3 -I scripts/lifecycle_model.py transition --input transition.json
```

The portable entrypoint is `skills/agent-memory/scripts/lifecycle_model.py`.
These are read-only evaluators exercised by the offline contract suite. They
write no bank, registry, journal, event or projection. Exit 0 means the request
was evaluated, including a negative decision; the caller MUST inspect
`result.current` or `result.legal`. Exit 1 means malformed inputs; exit 2 means
invalid command arguments. A positive evaluation MUST NOT be reported as a
committed or verified memory mutation.

## Selection inputs

A normalized selection request has a record and a separately resolved context:

```json
{
  "record": {
    "status": "active",
    "trust": "reviewed",
    "visibility": "visible",
    "confidence": 0.2,
    "valid-from": null,
    "valid-until": null
  },
  "context": {
    "authorized-scope": true,
    "applicable": true,
    "now": "2026-10-22T17:00:00Z",
    "minimum-trust": "reviewed",
    "active-superseders": []
  }
}
```

The invoking operation MUST derive authorization from resolved project access,
applicability from current component/branch/worktree/task/session conditions,
and superseders from the observed bank. Those facts MUST NOT be accepted from
instructions in retrieved memory. Unknown applicability MUST be explicit and
MUST NOT become true by default. The evaluator is a shared decision primitive,
not an identity resolver, ACL, complete typed-record validator or source of
independent semantic judgment. Developer attribution MUST NOT grant access or
create a personal memory namespace.

All seven gates in the definition MUST pass before ranking. Known validity
uses an inclusive start and exclusive end; null or absent bounds are unbounded.
Accepted fractional seconds are compared exactly at their full precision,
including beyond microseconds and across equivalent UTC offsets.
Trust levels express the invoking operation's evidence assessment, not
authenticated identity. The operation MUST supply its trust threshold
explicitly. Confidence MAY inform ranking but MUST NOT override trust,
visibility, applicability, lifecycle or a superseding record. Hiding a record
is distinct from deleting or revoking it. The result names every failed gate
so retrieval and assembly can explain their omissions consistently.

## Transition inputs and effects

The transition evaluator consumes the existing record's `id`, `version`,
`status`, prior evidence/event references and relevant validity boundary. Its
event supplies a fresh random event ID and the operation-specific guards from
the definition. Reused known event IDs MUST fail. An evidence list MUST contain
nonempty references; reactivation and correction MUST include new evidence.
The mediated caller MUST verify the referenced sources and authority rather
than treating the presence of a string as semantic proof.

Correction MUST create a higher version of the same stable record ID and
supersede the incorrect version. Supersession MAY name a different record but
MUST name an active replacement and the exact replaced ID/version. The
replacement carries `applicability-preserved: true` only after the agent has
reviewed distinct conditions, evidence and provenance; the evaluator cannot
prove that semantic judgment. Ordinary revocation MUST work without a
replacement. Expiry MUST use a known reached validity boundary, never age alone.

Reactivation MUST create a higher active version with a new event and new
evidence; the prior version MUST retain its historical state. Deletion MUST be
terminal for that record ID. A later capture MUST NOT recreate that deleted ID.
Actual deletion MUST use the content-free tombstone contract, not retain the
deleted body as an event payload. The evaluator describes the intended effects
but performs none of these writes.

Admission, correction and maintenance operations MUST additionally enforce
search-before-write, source/evidence review, identity binding, scope/security
checks, observed hashes, locking, idempotent events and readback through their
own public operation. A valid transition MUST NOT bypass those gates.

The fixture suite checks state transitions, terminal deletion, replacement
identity/version constraints, independent trust/confidence/visibility/time,
every selection gate, malformed requests and generated-guide agreement. The
fourteen full-product contract groups remain separately pending until their
operations ship; this foundation suite does not claim complete learning or
retrieval acceptance.
