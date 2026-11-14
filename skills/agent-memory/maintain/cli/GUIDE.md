---
kind: operation
domain: maintain
---

# Use the common CLI

## Use when

A session SHOULD use this leaf to discover installed commands or translate a
documented public helper to the installed interface.

## Required context

A session MUST first load and follow the [maintain domain](../GUIDE.md).
It MUST route to the operation's owning domain and leaf before performing a
memory operation; this guide describes invocation, not a second memory policy.

## Inputs and preconditions

A session MUST identify its trusted package or installed receipt and the
explicit repository/request for its selected operation.

## Observe

A session MUST inspect the relevant current state through its owning operation
before preparing a write; a command name does not supply identity or consent.

## Plan

A session MUST keep the selected helper's proposal, evidence and read-back
requirements when using its installed alias.

## Apply

Installation creates the native `am` binary and the compatibility command
`agent-memory` as separately owned targets. Lifecycle and diagnostic commands
print concise human-readable results by default. A session MUST pass `--json`
when it needs the complete machine result, qualifications or receipt. Memory
operations retain their structured proposals/results. This pack requires the
`am` command installed by the bootstrap; `am --version` checks it. If the
installed command is missing, rerun the verified bootstrap before using:

```sh
am --help
am identity status --repo REPO --json
am init inspect --repo REPO
am search --repo REPO --request RECALL_JSON
am checkpoint inspect --repo REPO
am boundary inspect --repo REPO
```

The compatibility command `agent-memory` accepts the same arguments as `am`.
A session SHOULD use `--help` on the selected command to discover its exact
request/proposal arguments. Paths with spaces
MUST remain one quoted argument. Invalid arguments exit 2; a refused operation
exits nonzero and retains the helper's evidence/recovery instructions. A
successful preview is not an applied mutation or verified publication.

When recovery coverage binds exact future maintenance output, a session MUST
first use `compact plan` or `govern plan`, independently review the actual planned
files through the owning operation, then use `preview --plan` with that review.
`admission preview` retains its documented seeded review workflow. Unplanned
previews MAY accept `--operation-id UUID --recorded-at RFC3339` as CLI options,
not request fields; those seeds grant no source, authority or coverage credit
and MUST NOT replace the required planning and independent review.

Bare `am`, `am help`, `am --help` and `am -h` show public help. `am --version`,
`am -V` and `am version` show the executing version, revision/cleanliness and
source without memory setup. `--verbose` adds diagnostic detail. A session MUST
retain operation exit codes and MUST NOT infer success merely from readable
formatting. The default repeated one-line installer follows the guarded updater
for an existing managed installation; explicit target-reconfiguration options
retain the separately previewed installation operation.

| CLI group | Public command | Meaning |
| --- | --- | --- |
| `developer`, `project`, `identity` | `am developer`, `am project`, `am identity` | Explicit developer setup and repository binding |
| `init`, `inspect`, `bank` | `am init`, `am inspect`, `am bank` | Evidence-based initialization and file contract inspection |
| `bank recover-layout` | `am bank recover-layout` | Bounded raw-source inspection/review, transactional layout repair, durable pending units and known recovery; separate publication/retention |
| `search`, `get`, `explain` | `am search` | The same qualified recall engine; request text, IDs or paths and inspect inclusion/exclusion evidence |
| `discover` | `am discover` | Bounded meaning-based navigation and source qualification by the invoking harness |
| `context` | `am context` | Bounded, source-labeled context preview/check |
| `capture` | `am capture` | Private durable-learning candidate, not admission |
| `admission`, `compare` | `am admission`, `am compare` | Search/read existing memory, semantic judgment and governed admission |
| `compact`, `checkpoint` | `am compact`, `am checkpoint` | Plan/review/preview/apply condition-preserving consolidation or observed active-context checkpoint |
| `correct`, `supersede`, `revoke`, `forget`, `govern` | `am govern` | Evidence-bound governance plan/review/preview/apply |
| `semantic`, `publish` | `am semantic`, `am publish` | Review shared meaning and land memory independently of feature code |
| `refresh`, `boundary` | `am refresh` | Inspect/prepare/apply newly landed same-repository memory and qualified imported recall |
| `omp-boundary` | `am omp-boundary` | Private native source inspection and exact single-observation assessment, using ordinary approved tools; callbacks expose observation only |
| `sharing`, `exchange` | `am sharing`, `am exchange` | Git boundary inspection and exchange recovery/replay |
| `migrate` | `am migrate` | Explicit bundled format migration, recovery or verified completed-bank restoration |
| `projection`, `writes`, `transaction` | `am projection`, `am writes`, `am transaction` | Derived references and common observed-write primitives |

`correct`, `supersede` and `revoke` MUST carry the matching explicit governance
operation in both request and proposal. `forget` accepts explicit expiration,
retention or deletion requests; it MUST NOT infer deletion consent. The generic
`govern` group also exposes reactivation under the existing governance rules.

For new shared-memory work, follow the owning semantic/refresh/publication
guides' explicit private-output interfaces. Ordinary learning comparison uses
comparison-v2, including an explicit checked empty frontier; omitted frontier
fields or unfinished v1 proofs cannot authorize writes. Semantic-sharing-v2
remains a separate unchanged schema. A stdout summary is not the stored complete
manifest, judgment, proof or proposal. See the
[recovery workflow](../../bank/GUIDE.md#source-preserving-layout-recovery)
for all-source review, source pages, independent attestation and recovery states.

`am semantic inspect-v2 --pages-dir ABSOLUTE_NEW_DIRECTORY` optionally exports
the exact checked disclosure pages alongside its required new manifest output.
The [semantic guide](../../sharing/semantic/GUIDE.md) defines receipt/page checks,
private output restrictions and the independent fresh authorization boundary.
Reading that bundle does not rebuild the frontier or authorize a stale judgment.

### Natural turn boundaries

Timing follows the [shared skill definition](../../SKILL.md#natural-turn-boundary).

A session MUST use the [sharing domain](../../sharing/GUIDE.md) at natural turn
boundaries to refresh landed memory before admitting new learning. The `boundary`
group is an invocation alias for the same inspect/prepare/apply refresh protocol;
it does not invent a semantic judgment or capture facts automatically by itself.
The skill supplies automatic evidence-based capture, comparison, consolidation,
pruning and eligible publication through their existing operation leaves. A
session MUST inspect the actual result and route again after each state change.
No background process or harness callback is required for this workflow.

## Read back

A session MUST read the selected helper's complete result and its actual paths,
scope, operation and verification fields before proceeding.

## Verify

A session MUST require the operation leaf's successful exit and verified
read-back before claiming a governed change, publication or imported recall.

## Outcomes

A session MUST distinguish an inspection, private candidate, prepared proposal,
local checkpoint, committed transaction, publication and fresh imported recall.

## Failure handling

A session MUST preserve a refusal and pending evidence and follow its owning
operation's recovery path rather than retry with weaker authority.

## Diagnostic fallback

A session MUST use the [file-only operation](../../offline/GUIDE.md) when the
installed CLI or an independently trusted `am` binary is unavailable. Canonical
Markdown remains readable; the constrained manual checkpoint keeps its existing
single-file/no-concurrent-writer limits. A session MUST NOT claim mediated
authorization, admission, concurrency, audit, fresh exchange or verified receipts
from that fallback. Installing runtime and explicitly establishing identity do
not depend on a previously initialized bank.
