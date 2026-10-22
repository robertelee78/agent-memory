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
