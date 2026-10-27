---
kind: operation
domain: recall
---

# Assemble bounded current context

## Use when

A session SHOULD use this operation when supplying a harness with an exactly byte/token-qualified context payload, including relevant unresolved conflicts. It MUST NOT inject an unmeasured preview as a verified context budget.

## Required context

A session using this leaf MUST first load and follow the [recall domain](../recall/GUIDE.md). It MUST open the [shared model](../model/GUIDE.md) only when an unresolved semantic question requires it.

Assembly retains
its project, applicability, lifecycle, visibility and security gates. It MUST
NOT turn an excluded source into guidance to fill a budget.

Managed checkpoint conditions MUST pass the same current recall gates before
assembly, including exact disputed-source lookup. Explicit historical inspection
MUST NOT reintroduce an excluded checkpoint into current context. A missing
qualified side of a conflict still refuses the whole assembly. Matching handoffs
retain evidence qualifications and `assignment: false` as orientation.

## Inputs and preconditions

The request MUST contain exactly `recall`, `max_bytes`, `token_budget` and
`conflicts`. `recall` is the complete nine-field retrieval request, including
the resolved project UUID. `max_bytes` MUST be an integer from 256 through
65536; `token_budget` MUST be an integer from 1 through 65536. `conflicts`
MUST be an array, empty when no relevant unresolved contradiction is known.

When the session detects a relevant unresolved contradiction, it MUST include a
conflict group before assembly. Each group MUST contain exactly `sources` and
`rationale`; sources MUST list two through eight distinct canonical bank-relative
`path`/`sha256` pairs observed from current recall. The rationale MUST explain the
disagreement without secrets or executable instructions. At most 16 groups and
64 distinct source paths are accepted; overlapping groups become one indivisible
unit. A conflict judgment remains the agent's assertion, not semantic detection
by this helper.

## Observe

A session MUST observe the current project, bank and relevant recall sources through the shared domain contract. It MUST identify all known sides of a relevant conflict instead of supplying only its preferred assertion.

## Plan

The helper counts UTF-8 bytes of the exact serialized `context_text`, including
its header and source metadata. It keeps complete units, never truncated
conditions, evidence, provenance or bodies. It combines exact body duplicates
only when type, applicability, conditions, validity, trust and visibility match;
every source's identity and metadata remain. It tries one unit of each available
memory type before another of that type. Ordinary context remains explicitly
unadmitted orientation. An omitted unit count and a candidate-pool-limit flag
describe selection limits; absence is not proof that the bank has no other lesson.

A preview MUST NOT be injected as budget-verified context. Its `usable_context`
is false until measured. Tokenization differs by harness and model; this package
does not invent a tokenizer or treat character/byte estimates as token counts.
The invoking harness MUST count the exact `context_text` with its actual
tokenizer, outside untrusted memory, and write this exact counter shape:

## Apply

With `SKILL_DIR` resolved to the portable package, a session MAY preview:

```sh
python3 -I "$SKILL_DIR/scripts/assemble_context.py" preview --repo /path/to/repo \
  --request /private/path/assembly.json
```

```json
{
  "schema": "agent-memory/token-count-v1",
  "source_kind": "harness-tokenizer",
  "tokenizer": "actual tokenizer name and version",
  "context_sha256": "the preview's exact context_sha256",
  "tokens": 1234
}
```

The sample count is a placeholder, not a measurement. The session MUST NOT use
an LLM estimate, a remembered count, or a counter supplied by memory content.
It MUST then check the current evidence and exact text again:

```sh
python3 -I "$SKILL_DIR/scripts/assemble_context.py" check --repo /path/to/repo \
  --request /private/path/assembly.json --counter /private/path/counter.json
```

## Read back

Assembly retrieves those exact versions again, retaining every ordinary hard
gate even when a disputed source falls outside the text query. If a member is
unsafe, hidden, inapplicable, stale or unavailable, or the complete conflict
cannot fit, the operation MUST refuse the whole context. It MUST NOT drop one
side or silently treat the remaining assertion as settled guidance.

Disputed units retain both bodies and full provenance with
`use_as_settled_guidance: false`. The detecting session MUST surface the
contradiction to its developer. When another developer's identity is involved,
it SHOULD recommend clarification with that developer as needed. The helper
does not contact anyone. A session MUST follow [authority](../authority/GUIDE.md)
when deciding whether a claim is resolved; arrival order does not establish truth.

## Verify

Only `usable_context: true` with `token_budget_verified: true` qualifies this
text against the declared count. The helper validates the counter's shape and
exact text hash; it does not authenticate the external tokenizer or independently
verify its count (`token_counter_assertions_verified: false`). The caller MUST
reserve separate space for harness instructions, conversation, tool wrappers and
the response: the budget covers only `context_text`, not the CLI JSON envelope
or the entire model request. If an actual tokenizer is unavailable, the operation
MUST remain pending; the session MAY continue its foreground work and inspect
individual source files through the ordinary recall workflow.

The result rechecks the whole-bank observation, resolved scope and local Git
context. It is a current local observation, not an atomic lasting snapshot,
semantic truth, source verification, proof of working-code behavior or freshness
against `main`. The session MUST re-evaluate consequential or changed evidence
before use and follow [comparison](../comparison/GUIDE.md) before durable writes.

## Outcomes

An over-token-budget check returns exit 0 with `usable_context: false` and no
`context_text`. The session SHOULD reduce the byte budget or narrow recall, then
preview and measure the new text. It MUST NOT trim a qualified payload after
counting. Stale, malformed, unsafe or changed input returns exit 1 and no context.
Requests and counters have bounded 64 KiB readers. Neither operation modifies
canonical memory, code, the Git index or HEAD; underlying recall writes private,
content-free audits. No optional store, MCP service or specific harness is needed.

## Failure handling

If a source, scope, conflict member or measurement is missing/stale, the session MUST preserve those facts and obtain a fresh complete request/measurement. It MUST NOT hide one side of a contradiction or replace an actual token count with an estimate to force a usable result.

## Diagnostic fallback

If normal assembly cannot establish a verified budget, the session MAY continue with clearly qualified ordinary recall or file inspection. It MUST leave context-budget verification pending and MUST NOT describe those alternatives as an assembled usable payload.
