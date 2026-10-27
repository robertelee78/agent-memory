---
kind: operation
domain: capture
---

# Turn-boundary memory

## Use when

Use this route when a harness invokes the skill at a natural turn boundary.

## Required context

A session MUST first load the [capture domain](../capture/GUIDE.md). The invoking
agent makes semantic judgments; the CLI observes and validates state. This route
never replaces the discovery, comparison, admission, refresh or publication gates.

## Inputs and preconditions

Resolve the actual repository, developer, session and harness. Inspect `am status
--repo REPO`. If the bank or binding is missing, ambiguous or unavailable, preserve
it and report pending; never infer that memory is empty.

## Observe

At the start of a turn follow [sharing refresh](../sharing/refresh/GUIDE.md) to
observe and import eligible landed memory. Offline or conflicting results remain
pending; do not interrupt unrelated work or claim freshness.

## Plan

Before finishing, consider whether this turn produced a durable, sourced project
lesson. If none, do nothing. Do not store transcripts, secrets, unconfirmed proposals,
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
python3 -I "$SKILL_DIR/scripts/admit_learning.py" draft --repo REPO --input PRIVATE_JSON
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

**Background boundary acknowledgment.**

When a background Stop wake supplies a source-bound session, prompt, message UUID and
source SHA-256, verify that exact source before assessing it. After an explicit
no-learning assessment, write a private JSON object containing `session`,
`prompt`, `source_uuid`, `source_sha256`, `outcome` set to `no-learning`, and a
bounded `assessment` explaining the agent's judgment. For a published lesson,
set `outcome` to `published` and add `publication_request` containing the exact
original prepared request. Run `am claude-boundary --repo REPO --input PRIVATE_JSON`.
The helper independently replays the publication request against the authority;
it records the connection to the prompt as an agent assertion, not semantic proof; it does not mechanically prove the published lesson derives from that source.
A denied command, unresolved conflict, unavailable source or unverified landing
MUST stay pending. Never acknowledge incomplete work as complete.

**Same-turn acknowledgement.**

A supported prompt hook records a private hash of the actual user prompt, session
and turn before a synchronous Stop continuation. It does not retain prompt
text. If a source-bound continuation is unavailable or interrupted, the record
remains pending. Inspect the current repository’s retained identifiers and prompt
hashes with `am codex-boundary --repo REPO --list-pending`; this does not contain
the original prompt or prove source completeness. Resume the original session
and verify the actual source before assessment. After the actual original turn
is assessed, acknowledge through
`am codex-boundary --repo REPO --input PRIVATE_JSON`. Use the session ID,
original turn ID for both `prompt` and `source_uuid`, and the source SHA-256
provided by the Stop hook. The remaining fields and outcomes follow the background
acknowledgement above. The source-to-publication relationship and no-learning
judgment remain agent assertions; a witnessed publication does not prove either.
