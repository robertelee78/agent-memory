# Common identity resolution and binding

The portable [identity guide](../../skills/agent-memory/identity/GUIDE.md) defines
the public status, preview/apply, initialization handoff and recovery workflow.
`scripts/scope_binding.py` invokes the same implementation as a copied skill.
The common resolver composes the existing developer and project registries;
clients do not have independent identity algorithms.

Single-file and multi-file observed writes now pin a `scope_context` in their
proposal. Missing or changed resolution refuses apply before mutation. The
context is rechecked under the common bank lock. Format-only preview/inspection
and raw file use remain available without setup. Resolution is attribution and
namespace evidence, not authentication or admission of a lesson.

Scope binding previews the full registry/bank change, persists a private recovery
bundle for bank-changing operations, updates the registry first, then uses the
multi-file journal with `MEMORY.md` last. Both stores must read back before the
combined result is verified. Empty-bank creation returns an initialization-only
context and pending status until initialization supplies useful Markdown.
Registry-only success is never advertised as a completely bound bank.

Binding can be interrupted between stores or within the bank transaction. Its
original decision remains replayable; known operation receipts never reapply
over a later correction or newer binding. Finished whole-bank recovery bundles
are removed. Existing transaction topology, canonical namespace, content-free
marker, partial-hash and external-edit checks remain in force.

The contract suite exercises actual public commands in disposable homes/repos:
missing/conflicting identities, no-remote initialization, explicit independent
forks with typed record/tombstone namespace updates, remote rebind alias history,
developer changes between preview and write and during the bank lock, torn-write
recovery, stale read-set refusal, copied artifacts and ambient Git redirection.
These are foundation checks; full installation, admission, initialization,
publication and cross-machine release suites have separate delivery gates.

## Private enrollment during layout recovery

`scope_binding.require` remains the ordinary guard and refuses a transaction
marker. The internal `private_enrollment_context(repo)` factory returns a
nonmapping, nonserializable `PrivateEnrollmentContext`, not a `scope_context`.
It consumes the frontier owner's exact verified persistent hold, live project
and actual developer registries, checkout/settings/authority pins, private
ledger/journal/index binding and capable-runtime check. A removed hold with a
surviving active binding is not an empty frontier. `recheck_private_enrollment`
independently reobserves the same pins; a frontier-only change invalidates the
context even when canonical bank bytes are unchanged.

Only private enrollment, pending inventory and bounded notice/wake bookkeeping
use this context. Source assessment, completion, refresh acknowledgement and
canonical/publication proofs retain their ordinary authority checks. All
Codex/Claude prompt and stop boundaries use the same shared turn-source
namespace, as does the native omp consumer; no adapter has another identity
algorithm or a caller-supplied held-authority flag.

Held registration durably records a native-event-bound enrollment intent before
source/index publication. Source/index fsync and readback, intent transitions
and final successful return recheck the original context. Interrupted or late
unverified writes remain retained and pending. Exact retries reconcile the
same observation ID and source slot, never a digest-only deduplication. A
missing native event ID instead retains a bounded unidentified private input,
with explicit uncertainty and no observation/completion authority.

The shared lock order is lifecycle, bank source barrier, native transport
registration, source completion in stable namespace order, then bank write.
The source barrier is reentrant for shared operations; a shared-to-exclusive
upgrade refuses rather than deadlocking or dropping a sibling capability.
The held-enrollment contract suite uses actual checked filesystem state and
real hook processes; its synthetic event fields are not native-provider or
macOS/Linux release acceptance evidence.
