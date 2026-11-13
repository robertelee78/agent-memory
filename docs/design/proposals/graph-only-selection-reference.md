# Proposed frozen-contract amendment — graph-only current selection reference

Status: **PROPOSED; not approved, not a new freeze, no implementation authorized.** Intended decision owners: frozen-contract/schema owner plus independent technical review and operator approval. This file changes no C/S/ADR/task bytes. It is the concrete proposal behind design OD-04, not evidence that OD-04 is resolved.

## 1. Exact base and necessity

C is `/private/tmp/am-v15-delivery/g1-approved-policy-release64-proposal-20261112/files/docs/design/v0.0.16-graph-embedding-execution.md`, Git blob `6911f12ebf8bc863781208476e613fb2281875f8`, SHA-256 `c545433771a48e9ed63c177b339acfa404fcdcefc174227fe3cb70fb090cdfe4`. S is the adjacent `semantic-result-v1.schema.json`, blob `acf0637ffe1ffba015607f7845a869d4edc119d1`, SHA-256 `b7d11e2011e245b336365e7bd1977e05f05676acb802f982ff4df73f20e7c41a`. Task T is `/private/tmp/am-v15-delivery/remaining-execution-triage-20261112/issue-257-body-20261113.md` L29/36/45–47/54/77/89. No unfrozen b1770bf source is used.

C §6 L483–487 requires a reference; C §13.1 L1904–1907 specifies only semantic-mode issuance and says literal output supplies none. Absent #256 means no qualified semantic profile. A fabricated token produces invalid-reference, which precedes profile-disabled under §12.1; that is not meaningful public missing-provider acceptance. T:36/45/46 and the public-related portions of T:47 therefore lack a #256-independent installed journey. This proposal adds one native **selection-only** route; it does not score, return guidance, waive source authority or change literal behavior.

## 2. Exact additive C text proposed

Append this new **§6.2 after the complete existing §6.1**, immediately before C §7's heading (after L537, before L538). Retain C §6 L497–516 (bounds and shared-scope paragraph) and **all existing §6.1 text byte-identically in their present sections**. No earlier heading is inserted after L496; historical seed refusal and closed related-query text stay in §6.

> ### 6.2 Native current-source selection for graph-only exploration
> `am graph select --repo R --request REQUEST` MUST be implemented by #257 as the selection-only public surface. REQUEST MUST conform to `agent-memory/graph-selection-request-v1` below, with exactly `schema`, `project_id`, `selector`, and `context`. `selector` MUST be exactly one of `{"id":UUID}` or `{"path":CANONICAL_BANK_RELATIVE_PATH}`. Unknown keys, mixed selectors, wrong types, invalid native UUID/path/context syntax, or a supplied Git branch/revision that fails the existing typed-context rules MUST produce the applicable §12.1 invalid-request or foreign-scope outcome. It MUST NOT accept query text, ranking/profile/fallback, a supplied source hash/version as authority, arbitrary Cypher, or a caller-supplied qualification witness.
>
> The operation MUST resolve only the currently authorized selected project/bank/checkout through native authority and any authentic pre-existing invoking binding; the #200/#253 general bound-session/plain-terminal origin mechanism remains with those owners. A request flag or claimed operator origin MUST NOT override an observed binding. It MUST use the same complete fresh native source qualification as related: all native kinds, minimum trust reviewed, context included, complete superseder inventory, security/visibility/lifecycle/applicability/scope/frontier/source gates, and exact generation-key/epoch/inhibit checks. Unknown required applicability/disclosure MUST yield clarification-required; an unreadable/incomplete required qualification seam MUST yield unavailable/graph-unavailable. Proven hidden active replacement of the selected otherwise disclosable obsolete source MUST retain generic withheld without replacement metadata. The shared qualification is source-only, owned by G3 for this selection operation and consumable by G2's ranking seam; no vector or similarity is consulted.
>
> Exactly one freshly qualified current RecordVersion matching the exact selector is REQUIRED to issue a reference. The `invalid-related-seed` trigger MUST be observed **only after complete source-only qualification definitively establishes no current disclosable RecordVersion target** (including a definite nonexistent/deleted/hidden/quarantined/unsafe/historical/context/index-only target), **and only when no selected-source withheld or clarification condition under C §8 L770–788 applies and required qualification is available**. If those withheld/clarification conditions apply, this particular refused trigger is **not observed at all**; retain their generic outcome with no token. If required qualification cannot complete, absence/noncurrentness is unproved and this refused trigger is likewise not observed: report unavailable/graph-unavailable (or the actual higher independent trigger). Therefore no exception to §12.1 status precedence is introduced. A definitively ineligible target yields refused/invalid-related-seed with no source-specific disclosure/token; other independently observed authority/input/erasure triggers still retain normal precedence. More than one otherwise valid current match MUST yield clarification-required, not an arbitrary version choice. A path-focused retry can disambiguate. A successful reference MUST bind the exact source key/path/hash, namespace, generation, privacy epoch, authority, profile and effective context under §12's existing source-selection purpose; it MUST NOT bind its emitting operation. Every later related request MUST independently requalify it and retain existing foreign/invalid/stale/invalid-related-seed rules.
>
> `graph.select` MUST NOT require an embedding profile/provider, embedding API contact, vector scoring, a query embedding, literal fallback, or #256 completion. It MUST NOT start/acquire/restart graph or model services. It requires a matching complete currently qualified graph generation and explicit running graph envelope; stopped graph → unavailable/runtime-stopped, no matching graph → the observed stale/source-stale or unavailable/graph-unavailable. A configured combined vector profile remains subject to §4's **complete** key equality; this amendment supplies no graph-only freshness exception while required vector fill is pending. Independent vector capability MUST be not-applicable because none was requested/assessed.
>
> On qualified/completed (exit 0), response MUST be `semantic-result-v1` operation graph.select with requested_mode selection, effective_mode selection, semantic_status not-requested, semantic_reason_code null, semantic_coverage not-applicable, actual fresh native_coverage and source_currency complete, graph_capability available, vector_capability not-applicable, a nonempty top-level selection_ref, results/mandatory_groups empty, next_page null, purge null, mutated false and resource_changed false. It MUST contain every other required S field using actual observed identity/resource/generation/profile evidence and §12.2 defaults; no source body or primary-guidance match is emitted. Remote freshness is not-applicable. The token is a disclosed selection locator, not verification of canonical truth/admission or primary guidance.
>
> All nonqualified outcomes MUST retain §12.1's full observed-trigger precedence/tie-break and exit 1 (invalid-request exits 2), effective_mode none, selection_ref null, empty results/groups, no continuation, no provider contact and no native literal switch. Invalid-request MUST retain native_coverage null and empty exclusions; other safely observed native coverage/exclusions remain actual. Native incomplete recovery/held frontier MUST prevent a qualified complete selection, retain actual coverage and report at least partial/partial-coverage absent a higher trigger. No token is emitted on partial/withheld/clarification/unavailable/stale/fenced outcomes. §12's existing reason enums are reused; no no-match shortcut or new refusal reason is introduced.

Append one public command bullet to C §12 after the structured related/search bullet:

> `am graph select --repo R --request REQUEST` is #257's selection-only graph request in §6.2 and always emits one JSON object, including recognizable syntax errors, without requiring --json. Its operation is graph.select; graph.select has its own required selection-mode field defaults above, not lifecycle or semantic ranking defaults. It does not change search/get/explain dispatch or ordinary literal/exact/path request and response bytes, schemas, selector/error behavior, ranking-key rules, matching strategy, exits or import requirements.

Append one row to C §12's operation table:

> `graph select` | `qualified/completed (0): exactly one current native RecordVersion selected, authenticated reference issued; no guidance body` | `nonqualified (1): mapped erasure/refusal/approval/incompatibility/unavailability/staleness/withheld/clarification/partial; no token` | `invalid-request (2)`.

Append after C §13.1 L1904–1907 (do not rewrite its historical semantic-mechanism statement):

> A graph-only supplied-seed mechanism control MUST obtain its current reference through the separately installed graph.select operation in §6.2. This is **native exact selection**, not semantic paraphrase retrieval, and MUST be labelled separately. Literal retrieval-result-v1 still supplies no selection reference. Public #257 graph-only qualification MUST exercise graph.select followed by installed related, including missing/unqualified similarity-provider fencing; a private/core/test-only mint or invalid-reference refusal MUST NOT count as that journey. The qualified-but-stopped provider cell needs a real qualified provider candidate, not #256 task completion or a fake. Absent evidence remains explicitly unexercised. Its **G3-versus-joint-G2 delivery allocation is the separate pending OD-08 decision**, not enacted by this selection-reference amendment; until an explicit task/contract-owner refinement is approved, the frozen T:45/47 allocation still applies.

## 3. Exact additive S changes proposed

S's schema tag remains `agent-memory/semantic-result-v1`; the amended exact schema digest/freeze and updated consumers are mandatory before the new operation is callable. The change is additive by operation, **not accepted by an old validator with the closed old enum**; no backwards compatibility for new-operation consumption by that old validator is claimed. Existing operations retain their original constraints. Proposed edits:

1. Append string `"graph.select"` to `properties.operation.enum`.
2. Append string `"selection"` to `$defs.mode.enum`.
3. Add `$defs.graph_selection_request` exactly as follows (native semantic validation adds UTF-8-byte, path, context/security/observed-Git rules described in §2):

```json
{
  "type": "object",
  "additionalProperties": false,
  "required": ["schema", "project_id", "selector", "context"],
  "properties": {
    "schema": {"const": "agent-memory/graph-selection-request-v1"},
    "project_id": {"type": "string", "pattern": "^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"},
    "selector": {
      "oneOf": [
        {"type":"object","additionalProperties":false,"required":["id"],"properties":{"id":{"type":"string","pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"}}},
        {"type":"object","additionalProperties":false,"required":["path"],"properties":{"path":{"type":"string","minLength":1,"maxLength":4096}}}
      ]
    },
    "context": {"type":"object","additionalProperties":{"type":"string"}}
  }
}
```

Path is additionally limited to 4096 UTF-8 bytes and native canonical bank-relative path syntax; this does not expand existing paths. Existing native context validators and task limit/security screen apply.

4. Append these **three** exact entries to S.allOf; preserve all original entries unchanged. The third rule makes the new shared mode legal **only** for graph.select, so no existing operation is widened to selection mode:

```json
[
  {
    "if":{"properties":{"operation":{"const":"graph.select"}},"required":["operation"]},
    "then":{"properties":{
      "requested_mode":{"const":"selection"},
      "semantic_status":{"const":"not-requested"},
      "semantic_reason_code":{"const":null},
      "semantic_coverage":{"const":"not-applicable"},
      "vector_capability":{"const":"not-applicable"},
      "remote_freshness":{"const":"not-applicable"},
      "results":{"maxItems":0},
      "mandatory_groups":{"maxItems":0},
      "next_page":{"const":null},
      "purge":{"const":null},
      "mutated":{"const":false},
      "resource_changed":{"const":false}
    }}
  },
  {
    "if":{"properties":{"operation":{"const":"graph.select"},"status":{"const":"qualified"}},"required":["operation","status"]},
    "then":{"properties":{
      "reason_code":{"const":"completed"},
      "effective_mode":{"const":"selection"},
      "selection_ref":{"type":"string","minLength":1},
      "source_currency":{"const":"complete"},
      "graph_capability":{"const":"available"},
      "native_coverage":{"$ref":"#/$defs/complete_native_coverage"}
    }},
    "else":{
      "if":{"properties":{"operation":{"const":"graph.select"}},"required":["operation"]},
      "then":{"properties":{"effective_mode":{"const":"none"},"selection_ref":{"const":null}}}
    }
  },
  {
    "if":{"anyOf":[
      {"properties":{"requested_mode":{"const":"selection"}},"required":["requested_mode"]},
      {"properties":{"effective_mode":{"const":"selection"}},"required":["effective_mode"]}
    ]},
    "then":{"properties":{"operation":{"const":"graph.select"}}}
  }
]
```

5. Add no top-level field, no reason/exclusion enum, no path-row/related/query modification and no new primary/native payload variant. Graph.select returns no `$defs.result` or `$defs.native` body. The original invalid-request and exit/identity/coverage constraints continue applying. Complete-native coverage on qualified graph.select prevents an empty/held/incomplete selection being called complete.

## 4. Authority, failure precedence and installed journey

Request/schema validation precedes any effects; foreign current-project/observed-Git authority refuses without sibling access; inhibit/epoch/ownership/seam/source qualification are fresh before token and again before emission. Full §12.1 status order is unchanged: invalid-request, erasure-pending, refused, approval-required, failed, incompatible, unavailable, stale, withheld, clarification-required, pending, partial, qualified. Refused reasons retain their existing tie order. The seed-ineligible trigger is **defined to be false** while selected-source C §8 L770–788 withheld/clarification conditions apply or required qualification is unavailable; it is observed only for a definitive completed ineligibility finding. Thus refused is not selected and then overridden by lower-precedence withheld/clarification: the contradiction does not arise, and every independently observed higher trigger still wins normally. Graph.select never selects a similarity provider, so **absent #256 is not a graph.select error**. The subsequent related query requesting similarity, with a valid graph.select reference, reaches the real profile-disabled/runtime-stopped fence after existing reference/authority checks.

Installed acceptance sequence after adoption: explicit setup/start/build graph-only → graph.select exact current native id/path → am related with replacement-evidence/component-attempts/recorded-related → actual independently labelled DB/native source checks → request SIMILAR_SUGGESTION with absent/unqualified real provider state and get whole-request profile-disabled with vector unavailable/no primaries/no send/no literal shortcut. Later install an actual independently qualified provider candidate, keep the graph running and independently stop/terminate only its exact verified owned resident vector worker (real supported control or an authorized real-worker fault), verify exit, select the current source under the matching complete generation, and obtain runtime-stopped without implicit restart. A separate public graph-stop journey verifies bounded actual hook shutdown/release. No direct API/test-only minted token substitutes for the first call; no supplied exact seed counts as paraphrase retrieval.

## 5. Alternatives and consequences

- **Recommended:** separate graph.select, body-free selection mode and explicit schema operation. It keeps ordinary literal bytes unchanged and gives public graph-only qualification a real first step. Cost: additive command/mode/schema/consumer/native-qualification/acceptance work needs independent contract-owner review/refreeze and operator approval; it is not already authorized by frozen C.
- **Literal exact-read opt-in:** could return references, but C §8 forbids new literal keys and C preserves literal envelopes. It still needs a reviewed contract/schema amendment, risks changing native compatibility, and is not a smaller silent patch.
- **Require semantic #256 issuance:** violates the required #256-independent local graph seam/journey; not recommended and would require explicit task-scope change, not a design workaround.
- **Private/core/test-only mint or accept invalid-reference as the “higher observed” provider status:** does not demonstrate installed public provider fencing/typed historical/navigation criteria. It is not acceptable acceptance evidence.
- **Unreferenced path/UUID passed directly to related:** changes the seed/authentication contract and risks forged authority; not recommended. Exact selectors belong only to fresh native selection before token issuance.

If this amendment or a complete reviewed alternative is **not adopted**, G3 can still discuss/implement independent lifecycle internals after its proper gates, but the current task's required public graph-only acceptance cannot be completed and #257 MUST NOT be marked fully accepted or delivered. The implementation-Ready gate in design §15 remains unresolved; no approved frozen source is edited or #255 research reopened by this proposal. Normal independent contract/schema change review and source-bound freeze/reconciliation, under the contract owner's authority, are required before treating any proposed text here as normative input.

## 6. Separate product consequences, not silently adopted by this amendment

Graph.select is read-only and adds no signing prompt. The prerequisite graph setup/start workflow is the separate OD-07 operator choice: the current engineering proposal uses administrator enrollment plus hardware-verified explicit signing; C §12's native-permission alternative remains permitted but deferred for that initial profile, not silently removed from C. OD-02 separately needs a first bounded measurement-profile/budget approval and later operational namespace-scaled resource approval. OD-08 separately decides whether real-provider stopped/hook cells delay G3 delivery or transfer, explicitly unexercised, to #256 joint acceptance. This document proposes none of those choices as already approved, changes no literal bytes or default status precedence, and does not waive any current task criterion.
