---
kind: operation
domain: capture
---

# Turn-boundary memory

## Use when

Use this route when a harness invokes the skill at a natural turn boundary.

### Routine compact path

Reuse this guide and the skill while their loaded content is unchanged. For an
initialized, bound bank, assess the actual source before ending. When no durable
candidate exists, use exactly one maintenance call, with no private request
workspace and no separate status, refresh, inventory or novelty scan:
`am codex-boundary --repo REPO --stdin --refresh --quiet`, or
`am claude-boundary --repo REPO --input - --refresh --quiet`.
Pass the exact supplied session, prompt, observation_id and source_sha256,
`outcome: no-learning`, and an authored `assessment` with observation ID as
source_id, actual source_review and truthful candidates. These are judgments,
never placeholders or a mechanically inferred absence of learning. The bounded
stdin object is limited to 64 KiB. The complete no-learning shape is:

```json
{"session":"ACTUAL_SESSION","prompt":"ACTUAL_PROMPT","observation_id":"ACTUAL_OBSERVATION","source_sha256":"ACTUAL_SOURCE_DIGEST","outcome":"no-learning","assessment":{"source_id":"ACTUAL_OBSERVATION","source_review":"Actual review of this original source and why no learning is eligible","candidates":[]}}
```

Replace every illustrative value with the actual supplied source and your own
review. `assessment` requires `source_id`, `source_review` and `candidates`;
category/durable/reason fields are not this contract. Candidate dispositions use
the evidence rules below. Multiple current observations may use
`{"schema":"agent-memory/turn-check-request-v1","assessments":[...]}`;
retain their distinct IDs and one actual session/lifecycle.

The combined `agent-memory/turn-check-v1` response keeps mechanical freshness and
source assessment separate. Successful quiet checks emit nothing. A real source
assessment can finish while freshness remains pending: overall verification is
false and exit status is nonzero. Report one brief actionable failure, then
preserve pending work; do not repeat the same repair narrative on every turn.
Verbose mode retains both stages for diagnosis. Native responses with
`notice_required: false` retain the pending state without another user-facing
notice for the same failure in that session. Older assessment-only receipts
retain their original narrow meaning and never prove freshness or publication.

The `codex-boundary` and `claude-boundary` adapters MUST emit the task answer or question only once, in final;
never emit the full answer first in commentary. The `opencode` adapter’s existing native
queued control pass follows the completed original answer. It MUST retain that
answer and MUST NOT repeat the business answer or emit ordinary bookkeeping
prose. With actual original source/context available, use one native
`am.source_ack`; it observes the source and checks freshness internally. Routine
success returns empty content. Its explicit `diagnostic: true` input returns
structured stages and original IDs for diagnosis, not ordinary bookkeeping. Read
`am.source_window` separately only for omitted/unknown evidence or recovery.
After an intentionally empty successful native acknowledgment, end the control
pass immediately: no further tool call or completion prose. A fallback after an
already emitted answer MUST preserve that original record without repeating it.
Some host SDKs may expose the final control record instead of the original task
answer; that host attribution limit MUST remain explicit rather than being hidden
by a duplicated answer.
Eligible or ambiguous learning MUST follow the governed route below, never a
fabricated no-learning shortcut. Native tool cards may remain visible.

## Required context

For a durable candidate, a session MUST first load the [capture domain](../capture/GUIDE.md). The invoking
agent makes semantic judgments; the CLI observes and validates state. This route
never replaces the discovery, comparison, admission, refresh or publication gates.

## Inputs and preconditions

The compact routine above does not need an extra status command. For durable
capture or recovery, resolve the actual repository, developer, session and harness. Inspect `am status
--repo REPO`. If the bank or binding is missing, ambiguous or unavailable, preserve
it and report pending; never infer that memory is empty.

## Observe

Before relying on a newly landed lesson follow [sharing refresh](../sharing/refresh/GUIDE.md) to
observe and import eligible landed memory. A routine no-learning turn uses its
combined quick check above rather than an extra refresh command; a review-required
result still needs the governed refresh route before claiming imported freshness. Offline or conflicting results remain
pending; do not interrupt unrelated work or claim freshness.

## Plan

Before finishing, consider whether this turn produced a durable, sourced project
lesson. If none, use the compact source acknowledgment above without admission. Do not store transcripts, secrets, unconfirmed proposals,
duplicate tracker status or personal memory. Read actual evidence and follow
[admission](../admission/GUIDE.md), [discovery](../discovery/GUIDE.md) and
[comparison](../comparison/GUIDE.md). Check alternate wording and relevant routes;
classify every plausible match by meaning and preserve conditions, evidence and
provenance. Conflicts and uncertainty remain pending and reach the detecting
person. A text search is not semantic proof.

## Apply

For one candidate, `am admission start --repo REPO --input PRIVATE_JSON` combines
mechanical draft, capture and the first discovery observation. Supply one object:

```json
{
  "task": "Find prior decisions about this actual subject and its conditions",
  "searches": ["actual concept", "different wording for that concept"],
  "candidate": {
    "body": "# Actual lesson\n\nThe small durable lesson and its conditions.\n",
    "kind": "project-decision", "scope": "repository",
    "applicability": "known", "conditions": {},
    "source": {"kind": "user", "actor": "actual source actor"},
    "evidence": ["Actual evidence identifying the source and relevant context"],
    "harness": "actual harness", "session": "actual session",
    "code_state": "not-code", "repository_evidence": null,
    "assessment": {"category": "confirmed-decision", "utility": "Actual future utility",
      "durable": true, "evidence_confidence": "confirmed", "evidence_reason": "Actual basis",
      "confirmed": true, "source_redundant": false, "work_state_copy": false, "review": null}
  }
}
```

Replace **every** illustrative value with the actual observed source and agent
judgment, including kind, source class, conditions, code state and assessment;
consult [admission](../admission/GUIDE.md) for valid categories and evidence.
This example is not evidence that an observation is confirmed or code-independent.
For an operator statement, use the explicit configured developer identity or a
non-identifying source label such as `operator`. In the actor, body, evidence and
all other fields, do not infer or copy a personal email, account profile or
contact details from harness authentication. Retain such details only when
actual project evidence requires them and existing sharing rules permit them.
Keep inputs and outputs private. For an initialized, bound repository, `am private
create --repo REPO` creates an owner-only workspace outside the checkout and
returns its ID and path. To retain an explicit authored JSON object without a
separate shell directory-creation command, pass its exact bytes on stdin to
`am private put --repo REPO --workspace ID --name request.json`. Use the returned
path as the input to the governed operation. The helper never supplies judgments,
replaces a prior file, or grants permissions; if the harness denies the command,
retain the work and report it pending. The response's `capture`, `discovery` and
`comparison_request` are inputs for the next steps; its discovery is explicitly
incomplete. Select plausible routes and paths by meaning, then supply them in
`comparison_request.read_paths`. Pass the `discovery` result and comparison request
to `am discover read`, with a truthful coverage rationale. Use its checked request
and observation for the existing comparison review; preserve the exact returned
capture and proof for preview. An ineligible or unconfirmed candidate returns no
discovery or comparison request. This helper does not admit or publish learning.

The [admission helper](../scripts/admit_learning.py) is also available as the
installed CLI. `am admission draft --repo REPO --input PRIVATE_JSON` fills the random record ID,
bound project/developer IDs and timestamps. Supply a JSON object with exactly:
`body`, `kind`, `scope`, `applicability`, `conditions`, `source`, `evidence`,
`harness`, `session`, `code_state`, `assessment`, and `repository_evidence`.
Use actual source, harness/session, applicability and evidence; the assessment
fields and legal values are defined in [admission](../admission/GUIDE.md).

```sh
/bin/sh "$SKILL_DIR/hooks/python3.sh" "$SKILL_DIR/scripts/admit_learning.py" draft --repo REPO --input PRIVATE_JSON
```

Pass the returned `request` to `am admission capture`, then complete semantic
discovery and comparison, `am admission preview`, and `am admission apply`.
Do not skip a gate.

## Read back

A draft is not captured or admitted. An admission receipt establishes only a
local governed write. Follow [publication](../sharing/publication/GUIDE.md) for a
memory-only landing and check actual authority readback.

## Verify

A local draft, injected instruction, successful hook process or unverified
attempt is not a published memory. Retain operation IDs and verified receipts;
distinguish local admission from actual landed publication and imported refresh.

## Outcomes

Conflicts, failures and pending publication reach the detecting person promptly;
ordinary success and no-learning outcomes remain quiet. The agent cannot assert
semantic truth or authority freshness from a mechanical draft.

## Failure handling

The adapter only supplies invocation context. The agent can still fail to follow
it; no hook proves that capture or refresh occurred. Report unsupported adapters,
missing trust, model failure and interrupted operations as pending.

## Diagnostic fallback

Preserve the bank, feature work and pending operations. Custom harnesses may
invoke the same skill and CLI explicitly; never replace governed memory with an
empty bank or a fabricated receipt.

**Same-turn acknowledgement.**

Supported native input hooks retain each observed submission separately,
including identical follow-ups. Native turn IDs can cover several messages. The
local `observation_id` identifies one observation; it is not a native message ID
or proof of source completeness. The source envelope supplied to the agent includes
native `session`, `prompt`, `observation_id` and `source_sha256`. A session MUST
assess the exact original source before finalizing its substantive answer and MUST
NOT attribute later instructions to an earlier source. When no durable learning
exists, no admission, semantic novelty scan or memory record is required.

For each supplied observation, submit an explicit assessment containing exactly
`session`, `prompt`, `observation_id`, `source_sha256`, `outcome`, and `assessment`.
Use one JSON object per observation. A session SHOULD construct JSON with a
structured encoder and MUST NOT depend on shell-specific word splitting to
separate identifiers. Set `assessment.source_id` to that observation ID; use the actual source review
and candidate dispositions described below. For the `codex-boundary` adapter use
`am codex-boundary --repo REPO --stdin --refresh --quiet`; for the `claude-boundary` adapter use
`am claude-boundary --repo REPO --input - --refresh --quiet`. These bounded stdin routes
accept no-learning only (at most 64 KiB). A published outcome MUST use a private
input file and the exact original prepared `publication_request`; the helper
independently verifies publication. A completion for one observation MUST NOT
complete another. Exact retries are idempotent; modified or corrupt evidence MUST
remain pending. The direct native prompt callback is the mechanical source witness for new
observations; a later transcript row is not required. This does not prove the
agent read or correctly assessed the source. For recovery, the agent MUST obtain
the actual original text and context and independently match its digest; absent
or ambiguous evidence MUST stay pending. A native transcript locator is only a
recovery aid, never a reconstructed message identity.

Inspect retained work with `am codex-boundary --repo REPO --list-pending` or
`am claude-boundary --repo REPO --list-pending`. The `codex-boundary` inventory pages expose
`next_offset` and `next_legacy_offset`; use their values with `--offset` and
`--legacy-offset`. These private inventories contain identifiers and hashes,
not original prompt text or verified semantic judgments. A session MUST recover
the original native source and digest before assessing earlier work. Unavailable
or interrupted sources MUST remain pending without blocking unrelated tasks.
Legacy source-bound continuations retain their original `source_uuid` input
format; a session MUST NOT manufacture new observation coverage for old turns.

Valid completed observations allow a quiet Stop. If assessment is missing, an
adapter MAY request a bounded continuation; it MUST preserve the substantive task
answer, retain pending work and stop retrying at its durable wake cap. Restricted
tools, permission denials and missing evidence MUST NOT replace a review or task
answer with memory bookkeeping. Routine success and no-learning MUST be silent in progress and final answers;
a failure SHOULD report its specific pending state briefly. A session MUST NOT
narrate private workspace creation or no-learning bookkeeping as product progress.

The `opencode` adapter retains its native session/message identifiers and uses its existing
`am.source_window` and `am.source_ack` tools. The source acknowledgment internally inspects the exact
original native source window; a separate window call is needed only for missing context or recovery. A session MUST retain pending interruption evidence and complete
only a truthful assessment or verified publication. Native adapters MUST NOT
pretend their message IDs are another adapter’s observation IDs or infer completion from a
successful queue or injected instruction.

**Legacy background boundary acknowledgment.**

When a background Stop wake supplies a source-bound session, prompt, message UUID and
source SHA-256, verify that exact source before assessing it. After an explicit
no-learning assessment, write a private JSON object containing `session`,
`prompt`, `source_uuid`, `source_sha256`, `outcome` set to `no-learning`, and a
`assessment` object: `{ "source_id": "ACTUAL_SOURCE_UUID", "source_review": "what the agent reviewed and why no learning needs capture", "candidates": [] }`. List each potential durable claim in `candidates` as `{ "claim": "actual claim", "disposition": "not-durable", "reason": "actual reason" }`. For a duplicate already fully covered, use `disposition: already-recorded`, add `memory_evidence` identifying the actual memory read, and explain the comparison in `reason`. This evidence and the duplicate judgment are agent assertions, not mechanically verified. Eligible, ambiguous or unresolved claims and incomplete capture or publication MUST remain pending. This structured assessment remains the agent's assertion, not proof that no claim was missed. For a published lesson,
set `outcome` to `published` and add `publication_request` containing the exact
original prepared request. Run `am claude-boundary --repo REPO --input PRIVATE_JSON`.
The helper independently replays the publication request against the authority;
it records the connection to the prompt as an agent assertion, not semantic proof; it does not mechanically prove the published lesson derives from that source.
A denied command, unresolved conflict, unavailable source or unverified landing
MUST stay pending. Never acknowledge incomplete work as complete.
