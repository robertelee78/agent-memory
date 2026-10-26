# v0.0.3 static-review follow-up RCA

Fable (actual Claude Opus 5.5), GLM-5.3 and Kimi K3 independently reviewed source archive
`90087179dd47ba5c32866c8e226afd7c919f1460`. All three approve their stated static scope
and identify no release-blocking code defect. They ran no tests and did not
accept the complete reliability workload or release. Their raw harness reports
and exact source-copy hashes are retained under
`/tmp/agent-memory-v003/external-reviews-9008717`.

The following mechanisms are confirmed by source inspection. Actual public
reproductions and design decisions remain required before implementing a fix;
these are not claimed as newly observed user incidents.

| Finding | Source-confirmed cause | User consequence | Planned treatment |
| --- | --- | --- | --- |
| Publication loses private-state repair diagnosis | `memory_publication.land` catches all `ContractError` from `semantic.check`, including typed dispute-state failures, and returns generic reconciliation guidance | Missing/interrupted registry state can be described as changed sources, omitting the supported recover-holds step | Preserve recognized failure diagnosis and real recovery boundary |
| Capacity is described as incomplete history | `sharing_holds.registry_value` combines valid history-count excess with malformed history; its generic error cannot explain a genuine capacity limit. The aggregate store also has a fixed bound | A valid full registry may refuse further renewal with guidance that recovery cannot satisfy | Distinguish capacity from corruption; design truthful, evidence-preserving next steps |
| Legacy uninstall lacks update's repair path | Uninstall compares the complete source-directory fingerprint; update additionally verifies the recorded managed Git payload when only mutable Git metadata differs | A clean owned legacy source can be retained after client removal, even though metadata-only drift can be verified | Reuse the verified ownership/payload boundary, preserving unknown edits and client references |
| Hashing precedes resource validation | `bank_writes.snapshot` invokes `hashes`, which calls `Path.read_bytes` for each Markdown source, before `contract.validate` checks the bank | A corrupt or accidental very large file can consume excessive memory before a supported-format refusal | Enforce existing bounds during observation without weakening hashes or freshness |

None of these changes is silently folded into the current v0.0.3 candidate.
[ADR-0018](../adr/0018-repair-diagnostics-and-observation-bounds.md) proposes four
independently reviewable outcomes for the next release. They retain existing
ownership, source and same-repository boundaries rather than adding another
lifecycle or changing the accepted v0.0.3 workload.

Two other static observations do not justify new implementation scope here.
Fresh-install replay of an older valid signature requires control of the
HTTPS/GitHub locator trust boundary already documented; signatures authenticate
publisher bytes rather than independent freshness. User Git URL rewriting applies
to explicitly selected unsigned legacy/custom channels; the default signed
channel does not use it, and it does not claim publisher authentication for
custom Git sources. These limits remain distinct from the four repair outcomes.

Kimi's first review client reached its 20-minute deadline during read-only
inspection. A continuation of that same owned session produced its final verdict
without starting another investigation. The timeout and continuation remain
separate raw receipts. All three verdicts are static-scope approvals, not full
workload or release acceptance. Source-copy hashes remain unchanged after the
continuation.

Kimi also noted conservative optional-catalog handling of an oversized source;
actual consumers and the final authoritative inventory check already refuse or
exclude it, so no correctness defect or new task is inferred. A legacy-import
file-removal race fails closed before commit under same-UID concurrent mutation,
and custom Git clones have a time bound but no independent object/byte quota.
Those reported scope/hardening limits do not change the signed default channel
or justify a new storage architecture in this release. The cosmetic oversized
message spacing can be corrected with the bounded-observation follow-up.

The [exported reviewer reports and raw bindings](data/2026-10-26-independent-reviews/bindings.json)
retain each actual reviewed scope, finding and limitation.
