# Signed-cosine alternatives: measured numeric findings

## Decision supported by this experiment

Preserve signed cosine and keep an independent exact oracle. Do not promote the
existing native `similarity` field to a signed-cosine authority, and do not assume
switching from MAGE to core fixes it. The standalone USearch binding avoided the
observed sign conversion on the tested controls. Its fixed approximate search
also missed neighbors: changing the binding alone does not establish adequate
retrieval quality.

For the next AM-specific qualification, compare a correctly scored exact path
with optional bounded candidate generation followed by exact rescoring. Prefer
the exact path when its measured cost satisfies the actual AM workload budget;
approximation must earn its complexity with acceptable eligible-set recall.
This is a research recommendation, not a selected replacement product profile or
an amendment to the operator's separately recorded managed-core graph direction.
No production code or current AM safety/eligibility rule changed.

## Native sign-conversion mechanism

The retained core/MAGE diagnostic observed an antiparallel vector with native
`distance=2` and `similarity=1`, versus independent signed cosine `-1`. The native
exact cosine function agreed with the independent oracle. See the existing
[core diagnostic](data/2026-11-05-ssg-runtime/core-comparison/diagnostic-20261106-opus/report.json).

The primary [Memgraph v3.10.1 SearchNodes source](https://github.com/memgraph/memgraph/blob/v3.10.1/src/storage/v2/indices/vector_index.cpp)
constructs its result using
`std::abs(SimilarityFromDistance(metric, distance))`.
The [same-tag conversion helper](https://github.com/memgraph/memgraph/blob/v3.10.1/src/storage/v2/indices/vector_index_utils.hpp)
returns `1.0 - distance` for cosine. Consequently, this source converts a valid
signed result `-1` to `+1`. This is a source mechanism consistent with the actual
pinned-binary observation; the image was not independently rebuilt from that tag.

The absolute-value conversion happens **after candidate search**. It can corrupt
thresholding or downstream ranking based on the reported similarity; it does not
by itself prove that ANN candidate selection used absolute cosine. Recomputing
correct scores cannot retrieve candidates that were never returned. Storage-access
DDL failures and semantic/model/template misses remain separate problems.

## Executed comparison

Actual runtime: macOS27.2 arm64, existing CPython3.13.12, USearch2.26.6,
NumPy2.2.6, numkong7.8.6 and tqdm4.67.1. Exact wheel hashes and applicable dependency
closure were checked before isolated offline installation. No graph, embedding
endpoint, model or real bank participated.

The frozen design uses original contiguous f32 integer-vector bytes,64 dimensions,
eight queries per size, seed20261108, cosine distance, explicit f32 storage,
connectivity16, construction expansion128, search expansion64 and one thread.
Candidate budget32 is strictly smaller than every tested domain. The independent
oracle uses Python f64 values and `math.fsum`, with signed cosine and tie-aware
membership. Score tolerance2e-5 was fixed before execution.

Independent **Claude Opus5.5** reviewed the exact design/source before execution
and returned PASS for bounded research execution. One1k run was reviewed before
10k, then10k before50k; no tuning, retries or rebuild-until-pass. The initial
unexecuted design selected an unavailable3.11.11 interpreter and was revised
before this review. All three executed cells used the same reviewed source.

| Synthetic vectors | Build ms | Median exact query ms | Median HNSW query ms | HNSW candidate recall@10 range | Mean candidate recall@10 | Saved index bytes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 43.693 | 0.0698 | 0.0471 | 0.9–1.0 | 0.975 | 403,784 |
| 10,000 | 1,021.969 | 0.3384 | 0.1038 | 0.5–1.0 | 0.8125 | 4,044,788 |
| 50,000 | 8,567.414 | 1.4368 | 0.1726 | 0.2–0.8 | 0.5375 | 20,222,540 |

Every exact search contained the oracle top10 within its32 candidates. Both
exact and approximate raw distances matched `1 - signed cosine`; maximum observed
absolute error was below3e-8. Parallel, antiparallel, nonunit, orthogonal and
truncated-boundary tie controls passed. Invalid zero/NaN/infinity controls test
this experiment's pre-library input gate, not USearch's native rejection policy.

Real save/restore preserved returned candidate keys/scores in every measured
query. Each separate child refused a real index truncated to64 bytes with
`ValueError: Not a dense USearch index!`; no child crash or timeout occurred.
This is one corruption case, not general storage safety or an AM lifecycle test.

The external supervisor observed no1GiB process-group RSS or30-minute wall cap
violation. Sampled aggregate peaks were about49/69/167MB; short-lived peaks can
be missed. The normal busy host and concurrent full release gate were retained,
not treated as disqualifying. These single-run, eight-query medians are not p99s,
a causal cross-stack speedup, representative AM timings or proof of semantic
quality. Queries differ across sizes. Actual AM dimensions, embeddings, eligibility,
safety, deletion/fencing and lifecycle must receive their own qualification.

## What this changes—and does not

- **Signed-score correctness:** a path using the raw cosine distance correctly,
  or independent exact rescoring on original vectors, avoids the demonstrated
  absolute-value conversion. The actual frozen standalone path supplies a
  measured counterexample to the claim that antiparallel must score highly.
- **Approximate misses:** the fixed HNSW setting sacrificed recall at50k. No
  hyperparameter was retuned after seeing results. Exact rescoring cannot repair
  these missing candidates; a new bounded search policy needs separate reviewed
  calibration/evaluation.
- **Algorithm independence:** [Memgraph embeds USearch](https://github.com/memgraph/memgraph/blob/v3.10.1/src/storage/v2/indices/vector_index.cpp).
  This alternative is a binding/version/architecture comparison within the HNSW
  family, not evidence that an independent algorithm wins or that one library
  is universally state of the art. [USearch's documented metric/API](https://github.com/unum-cloud/usearch/tree/v2.26.6)
  and [hnswlib's explicit cosine-distance contract](https://github.com/nmslib/hnswlib)
  support the candidate shortlist; vendor performance claims are not adopted.
- **Operational separation:** a standalone vector index does not execute graph
  DDL and therefore does not exercise the same read/DDL lock interaction. It also
  introduces separate ownership, rebuild, provenance and disposal obligations;
  none are established by this numeric experiment. The original50k historical
  failure remains unchanged and its lock holder unidentified.
- **Semantic misses/serving drift:** no conclusion follows from random-vector
  recall. Correct numeric scores, correct rendering/model identity and useful
  relevant text are distinct requirements. Old exposed held-out labels remain
  exposed, not fresh independent validation for a changed policy.

## Reproducible evidence and limits

[Design](data/2026-11-08-vector-alternatives/design.json),
[exact experiment](data/2026-11-08-vector-alternatives/experiment.py),
[execution-review verdict](data/2026-11-08-vector-alternatives/execution-review.txt)
and [source-bound observations](data/2026-11-08-vector-alternatives/observations.json)
retain package/source/design/input/output identities, raw synthetic per-query
observations and explicit assurance limits. Per-size manifests and events are in
the same directory. The experiment expects its adjacent `design.json`; install
only the hash-pinned wheels in an isolated matching runtime and preserve every
attempt/output. Supervisor logs remain private diagnostics, with their measured
terminal resource summary projected into observations.

No AM integration, legal permission, fresh semantic/coding benefit, G1 acceptance,
new model/server selection or v0.0.15 delivery is claimed. This evidence informs
the execution contract and the next actual-profile comparison rather than
turning a faster synthetic number into a release pass.
