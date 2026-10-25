# systemPatterns

## Planning sources; approval and current applicability require review

- [docs/adr/0013-public-cli-and-corrective-releases.md](../docs/adr/0013-public-cli-and-corrective-releases.md)
- [docs/adr/0010-memory-bank-file-contract.md](../docs/adr/0010-memory-bank-file-contract.md)
- [docs/adr/0012-git-shared-memory.md](../docs/adr/0012-git-shared-memory.md)
- [docs/adr/0001-local-first-authority-boundaries.md](../docs/adr/0001-local-first-authority-boundaries.md)
- [docs/adr/0004-client-independent-integration.md](../docs/adr/0004-client-independent-integration.md)
- [docs/adr/0003-progressive-disclosure.md](../docs/adr/0003-progressive-disclosure.md)
- [docs/adr/0005-admission-consolidation-retrieval.md](../docs/adr/0005-admission-consolidation-retrieval.md)
- [docs/adr/0007-portable-pack-and-lifecycle.md](../docs/adr/0007-portable-pack-and-lifecycle.md)

## Reviewed intent: bounded-bank-map

The root memory-bank/MEMORY.md is a bounded router; useful context files link to stronger sources. Detailed memory loads only when relevant. Derived search indexes and caches are rebuildable; canonical Markdown routers and topic indexes remain shared sources.

Applicability: `{"conditions": {}, "scope": "repository"}`.
Reviewed value: `"Progressive disclosure over canonical Markdown"`.
Evidence: [docs/adr/0003-progressive-disclosure.md](../docs/adr/0003-progressive-disclosure.md), [docs/adr/0010-memory-bank-file-contract.md](../docs/adr/0010-memory-bank-file-contract.md).

## Reviewed intent: memory-only-exchange

Eligible memory lands through the repository’s existing Git rules independently of feature code. Turn-boundary refresh reconciles memory while preserving unpublished learning, code, index and feature ancestry.

Applicability: `{"conditions": {}, "scope": "repository"}`.
Reviewed value: `"Memory-only publication and refresh"`.
Evidence: [docs/adr/0012-git-shared-memory.md](../docs/adr/0012-git-shared-memory.md).

## Reviewed intent: comparison-and-contradictions

Capture reads existing plausible matches first; consolidation retains distinct conditions, evidence and provenance. Overlapping contradictions go to the detecting developer; landing order does not determine truth.

Applicability: `{"conditions": {}, "scope": "repository"}`.
Reviewed value: `"Read/compare before writing; clarify contradictory meaning"`.
Evidence: [docs/adr/0005-admission-consolidation-retrieval.md](../docs/adr/0005-admission-consolidation-retrieval.md), [docs/adr/0012-git-shared-memory.md](../docs/adr/0012-git-shared-memory.md).
