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

Installation creates `am` and the compatibility command `agent-memory` in the
receipt's `bin_dir`. Both wrappers are separately owned targets. Lifecycle and
diagnostic commands print concise human-readable results by default. A session
MUST pass `--json` when it needs the complete machine result, qualifications or
receipt. Memory operations retain their structured proposals/results. Packaged
Python helpers still emit JSON by default. The trusted source entry point remains usable before
installation or when repairing a missing wrapper:

```sh
python3 -I "$SKILL_DIR/scripts/installer.py" --help
python3 -I "$SKILL_DIR/scripts/installer.py" identity status --repo REPO --json
python3 -I "$SKILL_DIR/scripts/installer.py" init inspect --repo REPO
python3 -I "$SKILL_DIR/scripts/installer.py" search --repo REPO --request RECALL_JSON
python3 -I "$SKILL_DIR/scripts/installer.py" checkpoint inspect --repo REPO
python3 -I "$SKILL_DIR/scripts/installer.py" boundary inspect --repo REPO
```

The installed equivalents start with `am` (or `agent-memory`), followed by the same
arguments after `installer.py`. A session SHOULD use `--help` on the selected
command to discover its exact request/proposal arguments. Paths with spaces
MUST remain one quoted argument. Invalid arguments exit 2; a refused operation
exits nonzero and retains the helper's evidence/recovery instructions. A
successful preview is not an applied mutation or verified publication.

Bare `am`, `am help`, `am --help` and `am -h` show public help. `am --version`,
`am -V` and `am version` show the executing version, revision/cleanliness and
source without memory setup. `--verbose` adds diagnostic detail. A session MUST
retain operation exit codes and MUST NOT infer success merely from readable
formatting. The default repeated one-line installer follows the guarded updater
for an existing managed installation; explicit target-reconfiguration options
retain the separately previewed installation operation.

| CLI group | Existing public helper | Meaning |
| --- | --- | --- |
| `developer`, `project`, `identity` | `developer_identity.py`, `project_identity.py`, `scope_binding.py` | Explicit developer setup and repository binding |
| `init`, `inspect`, `bank` | `initialize.py`, `bank_state.py inspect`, `bank_contract.py` | Evidence-based initialization and file contract inspection |
| `search`, `get`, `explain` | `retrieve.py` | The same qualified recall engine; request text, IDs or paths and inspect inclusion/exclusion evidence |
| `discover` | `discover_memory.py` | Bounded meaning-based navigation and source qualification by the invoking harness |
| `context` | `assemble_context.py` | Bounded, source-labeled context preview/check |
| `capture` | `admit_learning.py capture` | Private durable-learning candidate, not admission |
| `admission`, `compare` | `admit_learning.py`, `compare_learning.py` | Search/read existing memory, semantic judgment and governed admission |
| `compact`, `checkpoint` | `consolidate_learning.py`, `checkpoint.py` | Condition-preserving consolidation or observed active-context checkpoint |
| `correct`, `supersede`, `revoke`, `forget`, `govern` | `govern_memory.py` | Evidence-bound governance preview/apply |
| `semantic`, `publish` | `reconcile_memory.py`, `publish_memory.py` | Review shared meaning and land memory independently of feature code |
| `refresh`, `boundary` | `refresh_memory.py` | Inspect/prepare/apply newly landed same-repository memory and qualified imported recall |
| `sharing`, `exchange` | `sharing_boundary.py`, `sharing_recovery.py` | Git boundary inspection and exchange recovery/replay |
| `migrate` | `migrate_bank.py` | Explicit bundled format migration, recovery or verified completed-bank restoration |
| `projection`, `writes`, `transaction` | `projection.py`, `bank_writes.py`, `bank_transactions.py` | Derived references and common observed-write primitives |

`correct`, `supersede` and `revoke` MUST carry the matching explicit governance
operation in both request and proposal. `forget` accepts explicit expiration,
retention or deletion requests; it MUST NOT infer deletion consent. The generic
`govern` group also exposes reactivation under the existing governance rules.

For new shared-memory work, the owning semantic/refresh/publication guides MUST
use their explicit v2 interfaces and private output artifacts. Legacy v1 proof
entry points remain supported for existing bounded callers. A stdout artifact
summary is not the stored complete manifest, judgment, proof or proposal.

### Natural turn boundaries

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

A session MUST use the [file-only operation](../../offline/GUIDE.md) when neither
the installed CLI nor its trusted packaged helper is available. Canonical
Markdown remains readable; the constrained manual checkpoint keeps its existing
single-file/no-concurrent-writer limits. A session MUST NOT claim mediated
authorization, admission, concurrency, audit, fresh exchange or verified receipts
from that fallback. Installing runtime and explicitly establishing identity do
not depend on a previously initialized bank.
