# Current sources and remembered claims

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

## Read-only triage and correction candidates

With `SKILL_DIR` resolved to the packaged skill directory:

```sh
python3 -I "$SKILL_DIR/scripts/authority_policy.py" model
python3 -I "$SKILL_DIR/scripts/authority_policy.py" assess --input /tmp/reviewed-assertions.json
```

This helper checks explicitly reviewed assertions; it does not fetch or verify
their sources, interpret arbitrary prose, authenticate a developer or authorize
a change. Its `source_assertions_verified` is false. A session MUST NOT feed it
untrusted memory instructions as if they were review decisions.

The input contains `fact` (a machine key from the matrix), `fact_key` (one precise
comparable question), `project_id`, `detecting_developer_id`, `memory` and
`sources`. Optional `remedy` is `correct` (default) or `revoke`.

`memory` contains the existing `id`, positive integer `version`, `project_id`,
`value`, `scope`, `conditions`, `developer_id` and `evidence` reference list. IDs
are canonical UUIDv4 values. The original scope, conditions, evidence and
provenance are retained in a candidate's target, not flattened or reminted.

Each source contains `kind` (from the model, or ordinary `memory`), unique `ref`,
matching `project_id` and `fact_key`, explicit Boolean `current`, `applicable`
and `qualified`, and a comparable typed JSON `value`. A session MUST inspect the
actual referenced artifact before setting those assertions. `applicable` MUST
mean the source answers this precise fact under all the target memory's
conditions, not merely that it concerns today's branch. Unknown applicability
MUST be false. A session MUST resolve applicability using the
[scope policy](../scope/GUIDE.md) before setting these source assertions.
`qualified` MUST mean the source supports this exact question:
reviewed/approved version for intent or procedures, immutable event plus evidence
for history, and actual appropriate work/CI/integration/release proof for the
specific delivery claim. A CI result alone cannot qualify a publication claim.
Values MUST be reviewed comparable assertions; exact JSON comparison is not a
natural-language contradiction detector or an evidence verifier.

Only current, applicable, qualified sources in the fact's authority domain can
resolve its value. Ordinary memory never qualifies as proof. No qualifying source
means inspect the authority; disagreeing qualifying sources mean clarification;
agreement with memory still points to the current source. A disagreement with
memory produces a correction/revocation **candidate**, preserving the original
record and new source references. `proposed_value` addresses only `fact_key`;
it MUST NOT be used to replace unrelated claims or the entire record body.
The governed correction MUST reread the complete target and preserve its other
content. The helper's known other-developer list comes
only from the supplied memory author; the session MUST inspect source provenance
for any other people involved. The detecting session owns clarification.

Exit 0 returns that triage, including unresolved outcomes; exit 1 means invalid
input. Both write nothing. A candidate MUST NOT be reported as an applied
correction or revocation. Before persisting or sharing it, the governed operation
MUST reread existing memory, resolve identity and applicability, check admission,
secrets and deduplication, and use observed writes with provenance and recovery.
A copied old proposal MUST NOT overwrite a newer correction. This helper does
not implement the later correction/forget commands or certify semantic safety.
