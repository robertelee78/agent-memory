# Design observation: literal recall cost at 1k, 10k and 50k lessons

Date: 2026-10-25. This is a narrow source-bound design experiment for #98/#99,
not a retrieval-quality, semantic-sharing, session-stress or release pass.

The actual published v0.0.1 source is
[63d3a64](https://github.com/robertelee78/agent-memory/commit/63d3a64c0e535148cc1b5deb9d67169dbbc7c525).
The comparison is the accepted migration candidate
[f07402c](https://github.com/robertelee78/agent-memory/commit/f07402c7f60ebbca488d8863fa14be7b8b8627da),
still declaring version 0.0.2; it is not a released v0.0.3 artifact. Neither version
was replaced, patched or given a fixture version override.

The exact producer is
[literal_growth_probe.py at 8d0cd23](https://github.com/robertelee78/agent-memory/blob/8d0cd231e6942e263adbdcad23847b44e61fe47e/scripts/evaluations/literal_growth_probe.py).
Its SHA256 is `6034aef1fc4dc6164fc3ca7ef6ca05e4afc05b0cbc90562607d20314281f6b80`.
[Structured observations](data/2026-10-25-literal-growth.json) retain the actual
source/artifact/fixture hashes, both repeated query timings and reader metrics.
The earlier preliminary producer was adapted to portable CLI arguments and
stronger byte/mode/actual-baseline checks, then this exact public producer was
executed again. Earlier timings remain separate observations.

## Observed result

On one macOS/Python 3.15.0 host, each version structurally validates the identical
deterministic bank and finds the same single intended record for the exact
ordinary-text query `baselinegrowth sentinel`. Distinct synthetic active lessons
use unique stable IDs and bounded hierarchical routes. These structural fixtures
are explicitly not evidence of actual semantic admission.

| Distinct lessons | Canonical files | Canonical bytes | v0.0.1 query seconds | Comparison query seconds | Reader candidate bytes per query |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 1,019 | 984,251 | 1.208–1.213 | 1.221–1.221 | 826,030 |
| 10,000 | 10,162 | 9,831,738 | 7.318–7.358 | 7.335–7.343 | 8,260,030 |
| 50,000 | 50,797 | 49,153,693 | 35.744–35.759 | 36.130–36.183 | 41,300,030 |

Each range contains only two sequential public-helper invocations. The selected
record object is 1,956 encoded bytes including metadata; its body is 86 UTF-8
bytes. This is not an assembled prompt/context measurement or a token budget.
The complete JSON result is about 4.1KB. No token estimate is supplied.

Candidate bytes count the reader's candidate scan. Validation, snapshot hashing
and other reads add work, so this is not total disk I/O. The complete bank grows
from about 1MB to 49MB while that scan grows from about 0.83MB to 41.3MB. The
observed literal lookup cost grows with bank size. These data establish neither
a candidate speedup nor statistical equivalence or general latency bounds.

## Reproduction and preservation

Use trusted clean checkouts of the two exact source commits. Run the producer
from this repository, supplying `--root` for the comparison checkout, `--baseline`
for the actual v0.0.1 checkout and `--out` for a new operator-owned private absolute
directory outside both sources. Default sizes are 1000, 10000 and 50000;
`--counts 1000` permits a smaller reproduction. Each later run is a new observation.

The producer verifies complete source package bytes/modes against Git, copies
each package into its own disposable home and invokes that version's actual
installation and public validation/recall helpers. The package is the installed
operator-selected development source; this is not a separate installed-wrapper
journey or a publisher-authenticated release install. Actual installation receipts
supply the recorded artifact digests. The common isolation harness comes from the
comparison source; it does not replace the baseline's product implementation.

It checks source/bank file bytes and modes, staged feature-file bytes, index bytes
and HEAD before/after the reader operations. It does not claim every filesystem
metadata field or zero filesystem writes: private security audits remain writes.
Validation records retain exit/timing/diagnostic and a stdout hash, not the full
validator response. Source code/labels and raw responses were retained privately
for independent evidence review; the public measurements contain synthetic data.

## What remains open

Binding, validation and inventory already touch the files before recall; this is
not a demonstrated cold-cache run. Version order is fixed and the corpus is simple.
One unambiguous literal target among benign distractors cannot establish paraphrase
quality, correct conditional or stale/correction selection, security coverage,
semantic contradiction handling or useful real-project recall. The 50k run is a
bank-size observation, not a 48-session concurrency/stress result. No Linux,
model/tokenizer or session publication/refresh experiment is included here.

The separate [sharing-capacity RCA](2026-10-24-growing-bank-sharing-capacity.md)
still blocks usable sharing at required sizes. Exact recall succeeding here does
not repair that path. The [proposed fixed evaluation](../design/v0.0.3-scale-evaluation.md)
still needs reviewed labels, a technical repair design, operator-reviewed numeric
tolerances and the actual required workloads. Both #98 and #99 remain open.
