# Checked recall observation: measured behavior, recovery controls and limits

The operation-local reader preserves the fixed labeled outcomes at 1k/10k/50k
lessons while its two local sequential samples measure about 0.84s/3.85s/17.9s.
The [published baseline](2026-10-25-labeled-recall-baseline.md) measured about
1.25s/7.38s/35.8s. The operator accepted a 25% median-reduction criterion and
preserving every passing case with zero unsafe/stale/incorrectly applicable returns
in the [target decision](https://github.com/robertelee78/agent-memory/issues/98#issuecomment-6402351109).
These are source-bound observations pending final candidate acceptance, not a
statistical, cold-cache or universal performance/quality guarantee.

The fixed 24-case suite still records 16 of 17 summed query-relevance hits, one
known raw-literal paraphrase miss, zero unexpected results and zero recorded
safety/source-contract failures at every size. Corpus/label hashes are identical
to the reviewed baseline. The selected literal payload remains 2,062 encoded bytes;
this is not assembled harness context or provider tokens.

| Distinct lessons | Published v0.0.1 seconds | Observation seconds |
| --- | --- | --- |
| 1,000 | 1.228, 1.273 | 0.835, 0.843 |
| 10,000 | 7.389, 7.363 | 3.818, 3.886 |
| 50,000 | 35.709, 35.868 | 17.869, 17.939 |

[Measured data](data/2026-10-25-recall-observation.json) retains exact samples,
source/fixture identities, cases, metrics and all 78 raw-response digests. The
measured implementation is `7f187e64cb03f0605b86e2570ec9b79e40c2d258`, native macOS
Python 3.15.0, 151 portable files, artifact
`4dc58014d51c700ccac5e9fce5cbe881d7d6c68c4826367f299414ec8adeb187`.
It still declares 0.0.2; no v0.0.3 release is implied. The existing public producer
`labeled_recall_probe.py` ran with candidate-only selection, unchanged labels,
four concurrent quality requests and two separate sequential samples per size.
The comparison is to actual published v0.0.1, not to a renamed modern algorithm.

## Why the implementation changed

A separately instrumented 10k public-reader profile found repeated full snapshots,
format validation and record parsing dominant. The repair reuses one invocation's
exact validated bytes and parsed metadata, then rechecks complete source hashes,
physical topology, markers, location, Git context and identity/scope before return.
There is no persistent search index, caller-provided validation authority or bank
format migration. Existing invalidation-before-query, trust/applicability, unsafe
replacement, orientation and bounded result policies remain.

Metrics distinguish logical candidate bytes inspected from the in-memory source
observation from three complete canonical payload reads. At 50k these report
178,252,062 canonical payload bytes and 152,394 canonical file reads. They exclude
repository discovery, Git, filesystem metadata and audit I/O and do not measure
total OS traffic. `parsed_records` includes optional record-like versions/history;
it is not a count of distinct learned facts.

## Defects found before landing and their corrections

The initial optimization at `5111b9031c401bef1986eaf5f81b0039eccef252` passed the
existing 20 recall/assembly methods but independent review found two missing
controls. A pure Markdown map omitted the physical-path check for contexts marked
absent, allowing an empty directory/FIFO at an omitted context name to look absent.
The fix checks actual omitted-path occupancy before accepting the observation.

A bounded marker read could still block on a FIFO inserted after the initial
identity check. The fix rejects an unknown present marker before reading it, and
requires an owned marker to remain present, ordinary, exact-token matched and read
through a no-follow/nonblocking descriptor. Source changes during initial/final
readback remain unavailable. These were pre-release implementation findings,
not failures attributed to the published baseline.

Registered public-command controls exercise omitted directory/FIFO occupancy,
actual source changes after read/final rendezvous, FIFO arrival at final readback,
and owned-lock loss/replacement during governed maintenance. Injected foreign
records and canonical edits are preserved; staged code/index/HEAD remain intact.
The original 23-method cohort ran on both macOS/Python 3.15.0 and Linux/Python
3.13.7, with all tracked source bytes/full modes and complete portable inventory
verified before/after. The additional owned-maintenance cohort at `f29f4a5` ran
36 registered methods per platform with no failures, errors, skips or expected
failures. Its only reader-code difference was an additional test rendezvous.
The final control reuses the existing transaction preflight rendezvous so the
four recall implementation files again match the measured `7f187e6` bytes exactly.
Source-specific results stay separate in the data; later freeze checks are separate.

## Actual agent reformulation, measured separately

The raw full-question miss stays in the literal curve. A separate actual Claude
CLI run received only that question and the literal-reader matching contract, with
customizations/tool access disabled and no bank contents, expected IDs, labels or
example answers. Its three generated queries were frozen before any reader call:

1. `retry transient network error`
2. `max retries backoff`
3. `retry limit attempts`

The first two returned no source; the third found the exact qualified retry lesson
at all three corpus sizes. No query was edited after seeing results. The actual
reader preserved canonical bytes, staged code, index and HEAD. The
[workflow data](data/2026-10-25-blind-query-workflow.json) records the prompt,
provider-reported model/usage, frozen query and response hashes and all outcomes.
This demonstrates one actual agent-assisted query-reformulation path, not a
scripted synonym oracle, direct full-question CLI semantics, autonomous answer
quality or general paraphrase understanding. Its cost includes three full reader
requests plus the model request; it is not the one-query latency in the table.

No Linux latency curve, cold-cache isolation, tokenizer-based harness assembly,
required #99 multi-session schedule or v0.0.3 delivery follows from these reader
observations. Source-bound independent acceptance and the full milestone release
remain separate gates.
