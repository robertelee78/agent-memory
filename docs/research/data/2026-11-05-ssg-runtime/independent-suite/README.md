# G1 independent paraphrase / typed-relationship / safety suite (#255)

Status: **labels and schema frozen** (`g1-paraphrase-relationship-safety-v1`,
authored 2026-11-05). This is research evidence for #255 G1, not a quality,
resource or latency pass. Numeric targets in `suite.json#/proposals` are
**proposed for operator freeze and unmeasured**.

## Independence

Labels were written from canonical fixture meaning, metadata, conditions,
routes and evidence, ADR-0017/0023/0024, and the approved #206 plan's
historical 16-question set. The author did not read any `/tmp/am-v15-delivery/scale-*`
report, #255 `qualify.py`, graph edge/score, vector, similarity value, or #255
model/retrieval output before freezing; one incidental two-line October v0.0.3
navigation summary seen while locating the historical cases is disclosed in
`freeze.json` and was not adopted. The validator executes no query-dependent
retrieval, embedding, graph or ranking code.

## Files

| File | Role |
| --- | --- |
| `suite.json` | Frozen machine-readable suite: sources/pins, vocabulary, resolved records, cases, mutations, global invariants, coverage map, proposals |
| `author_suite.py` | Authored case definitions; resolves labels to actual IDs/versions/paths/hashes and writes `suite.json` |
| `g1_relationship_overlay.py` | Suite-owned canonical overlay: episodic attempts, decisions, results, `derived-from`, shared evidence, unknown/task conditions, hidden/unsafe/injected endpoints |
| `validate_suite.py` | Light native validator (see below) |
| `validation-1000.json` | Actual validator report at 1,000 lessons, both variants |
| `--emit-oracles` output | Not committed (regenerable, ~0.6 MB at 1,000): expanded per-case required paths/IDs, forbidden primary paths and forbidden disclosure strings for the parent harness |
| `freeze.json` | Artifact hashes, source pins and independence attestation |

## Fixture and partitions

- Base: `scripts/evaluations/semantic_navigation_fixture.py` (`semantic-navigation-v3-parent-scopes`)
  at 1,000/10,000/50,000 lessons, unchanged; special records are size-invariant and
  also cross-checked against `labeled_recall_fixture.py`. 88 base cases.
- Overlay: base plus `g1-relationship-overlay-v1` (12 records under `src/ingest/*`,
  `src/billing/*`); 13 overlay cases. Base labels hold in both variants; the
  validator proves no overlay record is eligible in any base case.
- Finite context truth per case: supplied conditions, observed Git branch
  (default `main`), deferred/held paths, runtime state, mutation stage, and
  evaluation-time precondition. Unknown native facts are never generalized.

101 cases: 56 coding, 37 investigation, 8 orientation; 16 calibration (historical
16 seeds) and 85 held-out; 86 semantic-recall plus 15 typed-template cases
(`replacement-evidence`, `component-attempts`, `recorded-related`). Each case has
query/request, context truth, required/tolerated/forbidden primaries with expected
native exclusion reasons, mandatory caveats, required/allowed/disallowed typed
paths with reasons, recorded-versus-inferred allowances, disclosure and provider
send oracles, availability outcome/exit/status/no-match oracles, and rationale.
`suite.json#/coverage` maps 28 ADR/contract/assignment requirements to cases.

## What the validator proves

Against the given AM checkout: pinned source and native package hashes; native
`bank_contract.validate_files` of each variant/size; record id/version/path/file
hash/metadata; native `request_value`; per-record native eligibility for every
case context, cross-checked with `retrieval.evaluate_sources` exclusion counts and
selection (empty query); required/tolerated eligible and forbidden excluded for the
stated reason; exact-selector and literal controls through native `match`; lexical
overlap claims; caveats, recorded hops and disallowed-hop reasons against canonical
fields; native security/visibility/derived-from global invariants; mutation
application (correction, revocation, tombstone) through native validation; and
coverage completeness. It cannot prove graph/model/retrieval quality.

## Recipes

```sh
python3 -B author_suite.py --am-root AM --out suite.json          # reproduces suite.json
python3 -B validate_suite.py --am-root AM --sizes 1000 --report validation-1000.json --emit-oracles oracles-1000.json
python3 -B validate_suite.py --am-root AM --sizes 10000 50000 --crosscheck-sizes 10000 50000   # parent's reserved slot
```

Observed: 1,000 full run valid in 8.2 s; a 10,000 targeted subset
(`--cases c-journaling-7 r-journaling-component r-retry-co-routed ov-batch-attempts
h16-hidden-replacement`, native cross-check on) valid in 20.5 s. Full 10,000 and
50,000 validation were not run here (host reserved; ≤30 s smoke limit) and remain
an explicit prerequisite for the parent.

## Open contract decisions surfaced (not decided by this suite)

- Branch substitution: native refusal before content; `refused`(1) versus `invalid-request`(2).
- Semantic-mode result cardinality/cutoff: abstention scoring needs a profile cutoff
  frozen from the calibration split only.
- Embedding eligibility of inactive (superseded/revoked/expired) bodies for explicit
  historical views; the suite only requires they never appear as current guidance.
- Surface for route-pointer orientation answers (`o-route-request-reliability`).
