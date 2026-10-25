# Fixed labeled recall baseline before v0.0.3 repair

The completed local macOS experiment found the same qualified results at 1,000,
10,000 and 50,000 distinct synthetic lessons. Each 24-case cohort returned 16 of
17 authored query-relevance hits, with no unexpected returned paths and no
recorded safety/source-contract failures. The one missed hit is the deliberately
retained full-question paraphrase without shared literal terms. This establishes
a measured lexical gap and a stable initial quality curve, not universal recall
quality or agent semantic understanding.

The two sequential samples grew from roughly 1.25 seconds at 1,000 lessons to
35.8 seconds at 50,000. These samples describe this local run; they are not a
cold-cache, statistical performance or accepted release target. The fixed corpus
and labels were independently reviewed before measurement. No optimization was
used to alter the published v0.0.1 baseline or its declared version.

## Sources and experiment

- Actual published v0.0.1: `63d3a64c0e535148cc1b5deb9d67169dbbc7c525`, 138 portable
  files, artifact `5e57375c046062b268f7e19d84dd0edd67de46279789ac38d02710bc63d7995c`.
- Pre-scale-repair comparison and producer: `3aecb2adb425b78019156895fd61b11f0497d661`,
  150 portable files, artifact
  `45f3bac2fc8a2ec7deeb3ab335d5e86a23806b788b2e85e18afea02e14f743dc`.
  This source still declares 0.0.2; it is not a released v0.0.3 artifact.
- Native macOS, Python 3.15.0. Each source used its own public installed helper
  bytes in a separate disposable development-source World. This was reader
  qualification, not a signed-bootstrap or installed-wrapper lifecycle test.
- Every cohort performed separate public structural validation before queries.
  Two sequential timing requests preceded the four-worker immutable-bank quality
  cohort. Quality timings are contended and must not replace sequential samples.
- 198 recorded public commands include setup/validation and 156 reader calls:
  144 labeled quality cases plus 12 sequential samples. Raw stdout, stderr and
  execution/parse outcomes were retained before interpretation.
- Complete tracked source bytes/full Unix modes and physical portable inventory
  matched before/after. Each fixture bank, staged feature text, exact index bytes
  and HEAD was preserved. This does not claim every filesystem timestamp or
  private identity/audit file was unchanged.

[Full measured data](data/2026-10-25-labeled-recall.json) includes fixed labels and
fixture hashes, source/producer identities, selected paths, per-case checks,
metrics, raw-response digests and available/unavailable distinctions. The fixed
fixture and runnable producer are in `scripts/evaluations/labeled_recall_fixture.py`
and `scripts/evaluations/labeled_recall_probe.py` at the exact producer commit.

## Sequential observed elapsed seconds

| Distinct lessons | Canonical files / UTF-8 bytes | Published v0.0.1 | Pre-repair comparison |
| --- | --- | --- | --- |
| 1,000 | 1,020 / 1,185,137 | 1.228, 1.273 | 1.239, 1.241 |
| 10,000 | 10,163 / 11,866,170 | 7.389, 7.363 | 7.353, 7.401 |
| 50,000 | 50,798 / 59,417,354 | 35.709, 35.868 | 35.642, 35.963 |

Counts refer to distinct stable record identities; one corrected identity has
both a historical version and its current version. The 50k corpus therefore has
50,001 record versions. Body sizes at 50k range from 53 to 1,738 bytes, with median
177 and 95th percentile 210. The fixed-domain distractors contain separately
identified component observations and a longer tail; this synthetic distribution
is not a claim that every real repository has these record lengths.

## What the labels show

Exact ID/path, shared-term queries, multiple relevant lessons, qualified conditions,
corrected stable identity, distinct evidence and both unresolved wall-color claims
remain retrievable. Hidden/unsafe qualified replacements do not reactivate old
assertions; expired, revoked, superseded and unknown-condition/session guidance is
withheld. Orientation is labeled separately, without typed trust. Returned body,
complete metadata/version and canonical hashes are checked against actual sources.
Unsafe markers and protected source paths are checked in both output streams.

The 16 hits are summed across queries, not 16 distinct memories learned. Two
wall-color results make both claims available for detecting-agent clarification;
the reader has not detected or resolved their semantic contradiction. The staged
source differs from HEAD, and the result explicitly does not certify working-code
bytes. That case demonstrates branch-scoped labeling, not general working-byte
applicability verification.

The natural-language question is sent unchanged to the literal reader. The probe
never supplies a ground-truth synonym expansion or pretends a scripted oracle is
an agent. An actual agent-assisted question-to-concept workflow requires separate
measurement alongside this raw literal curve.

The selected encoded payload in the sequential literal request is 2,062 bytes.
This is not assembled harness context, token usage, full disk I/O or provider cost.
No tokenizer measurement was made. No Linux measurement, required multi-session
sharing schedule, quality/performance threshold, optimization acceptance or release
pass follows from these observations. Numeric targets require operator review
before a quantitative pass; #99 retains its separate useful-sharing obligation.
