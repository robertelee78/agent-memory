# #98 target amendment and timing evidence: supplemental approval

Reviewer: **codex-review-59**, independent of implementer **codex-root**. Existing work: `robertelee78/agent-memory:ADR-0005:growing-bank-recall`, requirement `7ca331173f75ccbd06d929647dc5bc68fe002c2c:ADR-0005:growing-bank-recall`. Candidate **8d2163bd3c6c5555aeaffc175bf2207917572dd8**, tree **00e4126294ae7bc73cd6ee88f5372c66cdf1b64d**, parent **e6ad11c6ccc5bd2a67c0f6dfe49557dcfa4b4346**.

**APPROVED as a bounded supplemental product-target/documentation and evidence amendment. No blocking discrepancy found.** This does not redo or replace the accepted d01ce2e whole-story verdict, promote a tracker phase, accept #99, or certify release delivery. No new workload, test, model or product execution was performed.

## Fidelity to the operator's decision

The supplied operator steering explicitly prioritizes precision and recall over speed, retains all three objectives, permits incremental speed improvement later, and moves the v0.0.3 warm target from under five to **under seven seconds**. The new ADR5 paragraph records that decision for the existing growing-bank outcome. It counts the **complete warm native navigation plus qualified read**, excludes **AI thinking time only**, and confines the numerical claim to the recorded local workload. It separately requires cold cost and retained attempts and rejects an every-query/every-machine guarantee.

The amendment does not change the accepted v0.0.1 pipeline or introduce a new semantic scope. It does not authorize narrower discovery, weaker source/safety/applicability validation, discarded misses/extras, or regression of the agreed fixed quality cases. The unchanged fixed-reader preservation/zero-safety-failure and at-least-25% comparison target remain separate requirements. The older literal miss, semantic extras, expired-source model/controller failure and original rejected 051 interpretation remain in the report. Plain terminal semantic interpretation remains the explicitly deferred follow-up; this timing amendment does not deliver it.

Under five seconds is now a later optimization goal, not the current release gate. Another quiet recheck solely to satisfy that obsolete budget is unnecessary. This is an authorized product decision, rather than inferring a waiver from measured times or from the desirability of shipping. It also does not waive #99's independently required useful sharing/recovery workloads.

## Independent raw/source audit

The diff is **19 documentation/data files only**. Runtime, tests and evaluation-controller bytes/modes are identical to frozen **81fec82c1619d20c12473d23e3fdf6706d52b641** and the e6ad parent. There is no runtime shortcut or changed label/query fixture in this amendment.

I independently checked both complete before/after source snapshots against immutable Git bytes/modes and their declared portable inventories. The repaired run is bound to **e6ad11c6ccc5bd2a67c0f6dfe49557dcfa4b4346** (797 tracked files, 162 portable); the comparison run is bound to previously accepted **d01ce2eb98b2669fee44d7c171239a183ceaf742** (710 tracked files, 161 portable). Both before/after maps are equal. Exported JSON files match retained originals. All **58 raw manifest file hashes/sizes** and **two retained producer-script hashes** match. The differing producer roots and source commits remain visible; they are not relabeled identical-source executions.

For all **24 logged public calls**, I checked actual zero exits, empty stderr, operation forms and parsed results. Each run performs start, two actual node calls and a qualified read, repeated once cold and twice warm. All six qualified reads return the same actual selected record/body/metadata/version/source hash across the two sources, with current/untrusted/provenance flags and the safe incomplete returned navigation trace. Raw qualifier metrics agree with the observations. Every full workflow timer includes all four operation durations plus its small intervening work; it is not merely the last read's time.

| Source-bound run | Complete warm native seconds | Complete cold rebuild seconds | Separate fixture setup seconds |
| --- | --- | --- | --- |
| Repaired runtime e6ad11c | **6.168920167 / 5.993992708** | **24.765108625** | 144.572375542 |
| Previously accepted runtime d01ce2e | **6.283692292 / 6.391442834** | **25.697891417** | 147.276841459 |

The new summary exactly matches those raw values. Both measured warm pairs are below seven; neither pair is below five. Both runs use the same semantic-navigation-v3-parent-scopes fixture statistics: **50,000 distinct lessons, 50,843 canonical files, 59,047,173 canonical bytes and 31 root routes**. Warm read metrics still report **59,048,144 canonical payload bytes / 50,844 file reads**, including full canonical hashing plus the selected-source work. They do not represent all filesystem/Git/audit I/O.

The producer asserts bank bytes/modes, index and HEAD preservation. Its broad `bank_feature_index_head_preserved` label does not include a separate final working-feature-file byte assertion; the staged feature blob is retained through index equality. Full product source preservation is separately bound by the complete before/after snapshots. This qualification is inherited from the known-path cost apparatus and is not inflated into independent observation of a removed World.

## What this evidence does and does not establish

These are local, sequential, **known-path** native timing samples, not another blind semantic test or a new fixed-quality matrix. Selecting the expected retry path in the controller isolates native cost; it cannot establish semantic precision/recall. The changed runtime's broader qualification, semantic observations and fixed quality/safety evidence retain their actual source-specific scope. No new perfect precision, zero-miss or generic relevance claim is supported by these two runs.

Concurrent heavy qualification load is a reported execution condition, not experimentally controlled utilization telemetry. The repaired samples are lower than the comparison samples in these observations; they do not isolate the repair's causal latency effect, prove load caused prior slower times, establish a statistical speedup, or guarantee the same ordering in another trial. Cold reconstruction and setup remain substantially above the warm target and are plainly disclosed. No cold filesystem-cache flush or supported-host latency matrix is claimed.

The report's sentence “show no measured slowdown from the repair” is acceptable only as that descriptive sample comparison. For sharper wording, prefer **“the repaired samples were lower in these runs; the repair's timing effect was not isolated.”** This is a nonblocking editorial caution, not a demand for another benchmark. Likewise the catalog document's retained earlier target paragraph should be read with the current full-path header and corrected rejection history; it must not revive the previously unsupported final-qualifier-only approval interpretation.

The release note continues to say that a published release and verified milestone delivery establish shipment; the branch/guide alone does not. Existing #98 acceptance/release-ready evidence remains distinct from this supplemental review. Current #99 runs, final source qualification, actual signed publication and delivery are not accepted here.

## Review artifacts and release

`source.json` binds all changed files, source manifests, timing values and source equality to frozen81. `evidence-manifest.json` and `audit.py` retain this reviewer's raw hash/check work; `audit.stdout` reports **zero discrepancies**. No product imports, tests/models, new workload runs, real runtime/cache/key/bank changes or external writes occurred. All own read/audit subprocesses are terminal and collected; no Worlds/background jobs exist. **All reviewer holds are RELEASED. External writer grant: NONE / RELEASED.**
