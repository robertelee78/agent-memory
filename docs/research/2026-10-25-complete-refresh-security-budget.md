# The last mutation gate still imposed a capture-sized refresh limit

Work: #99, `ADR-0009:daily-use-reliability`.

A focused public v2 control prepared and reviewed a valid incoming delta of
70 independent records in two bounded indexes. It exceeded both 64 changed
sources and 64 KiB, then refused at application on the existing `808260f`
runtime. The input was within the approved complete-bank limits. This is a
pipeline defect, not evidence that oversized or unsafe sources should be allowed.

The v2 snapshot, paging, private proposal and transaction readers already
supported complete banks, but the final `security.guarded_start` still reused
the small capture batch's 64-source/64KiB preflight. For example, importing the
other 23 sessions' new record, evidence and index can exceed that small batch.
Increasing only the outer input limit therefore could not deliver the workload.

The repair selects complete refresh scanning only after the locked refresh
validator has rechecked an **actual v2 source-bound proof**. It checks every
changed source and path label, retaining the complete 65,536-file/64MiB bank
budget and 16KiB source bound. Each source is scanned independently; nothing
is truncated, sampled or admitted merely because it fits. Legacy/capture and
other actions keep their existing small-batch rules. Complete capacity cannot
be selected for another action, or supplied as a new caller-controlled proposal
field. Existing identity, comparison, source, security audit and transaction
checks remain mandatory.

The public control requires the complete delta to import, compares every peer
record's bytes, verifies useful qualified recall, and preserves feature state.
A tampered final source must refuse without disclosure or canonical changes.
A separate explicitly labeled primitive control checks an unsafe final source
past item 64, aggregate overflow and misuse by another action; that unit control
is not substituted for public workflow evidence. The required multi-session
workloads and independent complete-story acceptance remain outstanding.
