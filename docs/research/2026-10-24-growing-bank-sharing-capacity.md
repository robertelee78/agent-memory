# Growing-bank sharing: design experiment and RCA

This is design evidence for [#99](https://github.com/robertelee78/agent-memory/issues/99)
and the separate [#98](https://github.com/robertelee78/agent-memory/issues/98)
evaluation program. It is not stress acceptance, a retrieval-quality pass, a
published release guarantee or an agreed numerical performance target.

## Observed public behavior

The experiment installed exact portable source bytes from
`dfc536e91819c082405278452557a7a7ee4ddace`, whose 142 portable files match the
accepted handoff candidate `0d583bb944607c18a53dbab68ef42813c5536637`.
It ran on macOS with Python 3.15.0, in disposable homes, banks, registries and
Git repositories with an isolated bare authority. It used native v1 fixtures,
the installed public bank validator, literal recall and semantic inspection.
Fixture records are not real admitted project learning.

| Distinct fixture lessons | Canonical files | Bank bytes | Exact literal recall | Semantic inspection |
| ---: | ---: | ---: | --- | --- |
| 8 | 12 | 9,284 | One expected record | Succeeded: 27 components, 351 pairs |
| 24 | 28 | 24,916 | One expected record | Refused; no source inventory returned |
| 32 | 36 | 32,732 | One expected record | Refused; no source inventory returned |
| 256 | 263 | 252,885 | One expected record | Refused; no source inventory returned |
| 1,000 | 1,019 | 984,993 | One expected record | Refused; no source inventory returned |

Every fixture passed complete structural validation. The experiment checked
that bank bytes, feature code, original index, HEAD and branch were preserved
across recall/inspection. Private audit and Git observation writes remain
distinct; this is not a claim of zero filesystem writes. All failed public
responses used the generic incomplete/changed/unsafe semantic-review diagnostic.
Missing inventory fields mean unavailable, not an empty bank or zero components.

The exact 24- and 32-lesson failure guard was not exposed by the public response.
These observations do not establish a universal 24-lesson capacity. Record size,
routing, source differences and metadata affect component/byte budgets. One
literal query returning one expected record is not a labeled quality suite or a
paraphrase/precision/recall result. No Linux, 10,000/50,000-lesson, session-stress,
model-token measurement or v0.0.1 comparison is claimed here.

## Source-level cause and coverage gap

[Sharing recovery](../../skills/agent-memory/scripts/agent_memory/sharing_recovery.py)
requires complete snapshots within 256 files and 1 MiB, with 16 KiB per file.
The 263- and 1,019-file fixtures exceed its file bound even though both are below
1 MiB. Safe refusal preserves the source but prevents the sharing workflow.

[Semantic sharing](../../skills/agent-memory/scripts/agent_memory/semantic_sharing.py)
builds components from complete local/landed sources, including unchanged
records, introductions and route prose. It requires at most 64 components and
64 KiB of encoded component data, then materializes every source pair. Pair
count grows as `n * (n - 1) / 2`; judgments require complete pair coverage, with
another 1 MiB proof/input bound. Raising a single limit does not remove the
quadratic review/payload cost or make the required growing-bank scenarios pass.

The public failure conflates incomplete, changed and unsafe review without
reporting which bound failed. Its suggested independent-unit review does not
itself provide a scoped/paginated complete-bank workflow. This is an actionable
diagnostic and workflow gap, not evidence that the data was corrupted.

Earlier small-bank contract passes cover their actual source and workload.
They do not exercise the planned 1,000/10,000/50,000-lesson envelope or prove
extended daily-use sharing. Local exact recall remaining available while
semantic sharing refuses is a different result from degraded recall quality.

## Repair design and acceptance still needed

The #99 design must address complete source inventories and bounded semantic
review without splitting one repository into independent memory banks,
truncating unseen knowledge or substituting a cache for canonical evidence.
It must preserve comparison before write/landing, cross-file contradictions,
conditions/provenance, invalidation and honest pending/freshness behavior.
Scoped or incremental review needs an explicit independently reviewed contract;
this report does not authorize weakening coverage by ranking away a conflict.

Add bounded failure categories and actual counters without echoing unsafe
content. Measure files, bytes, route/components, proof size, latency and recovery
effort separately. Reuse the existing local Mac/Linux qualification approach;
the required 24-session/10,000-lesson gate and the exploratory combined
48-session/50,000-lesson workload keep their distinct meanings.

#98 separately needs its fixed labeled query suite, actual v0.0.1 baseline,
quality/cost curves and operator-reviewed tolerances before an improvement or
performance pass. Storage/read capacity is not quality evidence. No new bank
format, personal memory, vector service, MCP or production migration follows
from this capacity finding.

## Reproduction and evidence

The [design probe](../../scripts/evaluations/sharing_capacity_probe.py) is
disposable fixture tooling. From a complete source checkout, run with Python's
bytecode guard and keep output outside the checkout:

```sh
python3 -B scripts/evaluations/sharing_capacity_probe.py --root "$PWD" --out /private/output/capacity.json
```

It verifies installed bytes against the selected Git tree, saves raw recall and
semantic responses alongside its summary, and records the actual source commit,
probe SHA256, platform and measurement limits. The observed producer script
SHA256 is recorded in its private summary. Reproduction at a later revision is
a new observation, not automatically the original result.

Independent source/evidence review matched all five measurement rows, all ten
raw responses and source limits. Initial fixture-version setup refusal and an
earlier summary's default-zero ambiguity were retained separately; neither is
presented as a product pass, product failure or successful proof of absence.
